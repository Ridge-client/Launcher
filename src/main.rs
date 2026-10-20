//! Ridge: OSINT catalog health checker and isolated Docker persona manager.

// A window app on Windows: no console window opens behind it. Command-line use attaches to the
// calling terminal instead (ui::init).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod annotate;
mod catalog;
mod cli;
mod graph;
mod gui;
mod persona;
mod photo;
mod sun;
mod ui;
mod update;

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use colored::Colorize;

use catalog::{health, Catalog, Entry};
use cli::{CatalogCmd, Cli, Command, PersonaCmd, StartArgs};
use persona::bookmarks;
use persona::docker::{Manager, StartOptions};
use persona::vpn::VpnConfig;
use persona::PersonaNames;

const MAX_TABS_WITHOUT_CONFIRM: usize = 10;

fn main() -> ExitCode {
    // No arguments (double-clicked, Start menu, or plain `ridge`): open the window.
    if std::env::args_os().len() <= 1 {
        return match gui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                ui::init();
                eprintln!(
                    "{} could not open the window: {err:#}",
                    "error:".red().bold()
                );
                eprintln!("Use the command line instead: ridge --help");
                ExitCode::FAILURE
            }
        };
    }
    ui::init();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("{} {err}", "error:".red().bold());
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(cli_main())
}

async fn cli_main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{} {err:#}", "error:".red().bold());
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Catalog(cmd) => {
            let catalog = Catalog::load(cli.catalog.as_deref())?;
            run_catalog(&catalog, cmd).await
        }
        Command::Persona(cmd) => run_persona(cmd).await,
    }
}

async fn run_catalog(catalog: &Catalog, cmd: CatalogCmd) -> Result<()> {
    match cmd {
        CatalogCmd::Categories => {
            for c in &catalog.categories {
                println!("{:>4}  {}", c.tools.len().to_string().cyan(), c.name);
            }
        }
        CatalogCmd::List { filter, json } => {
            let entries = catalog.filter(filter.category.as_deref(), filter.search.as_deref());
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
                return Ok(());
            }
            print_entries(&entries);
        }
        CatalogCmd::Check {
            filter,
            http,
            down_only,
            json,
        } => {
            let entries = catalog.filter(filter.category.as_deref(), filter.search.as_deref());
            if entries.is_empty() {
                bail!("no tools match the given filters");
            }
            let mut results = run_checks(entries, http.concurrency.into(), http.timeout).await?;
            results.sort_by(|a, b| {
                (a.entry.category, &a.entry.tool.name).cmp(&(b.entry.category, &b.entry.tool.name))
            });
            if down_only {
                results.retain(|r| r.status.is_down());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
                return Ok(());
            }
            print_results(&results, down_only);
        }
        CatalogCmd::Open {
            names,
            category,
            no_check,
            yes,
            http,
        } => {
            let mut targets: Vec<Entry> = Vec::new();
            for name in &names {
                let entry = catalog.find(name).with_context(|| {
                    format!("no tool named '{name}' (see `ridge catalog list`)")
                })?;
                targets.push(entry);
            }
            if let Some(cat) = category.as_deref() {
                let found = catalog.filter(Some(cat), None);
                if found.is_empty() {
                    bail!("no category matches '{cat}' (see `ridge catalog categories`)");
                }
                targets.extend(found);
            }
            if targets.len() > MAX_TABS_WITHOUT_CONFIRM && !yes {
                bail!(
                    "this would open {} tabs; pass --yes to confirm",
                    targets.len()
                );
            }
            open_tools(targets, no_check, &http).await?;
        }
    }
    Ok(())
}

async fn run_checks(
    entries: Vec<Entry<'_>>,
    concurrency: usize,
    timeout_secs: u64,
) -> Result<Vec<health::CheckResult<'_>>> {
    let pb = ui::progress_bar(entries.len() as u64, "Checking");
    let results = health::check_all(
        entries,
        concurrency,
        Duration::from_secs(timeout_secs),
        |_| pb.inc(1),
    )
    .await?;
    pb.finish_and_clear();
    Ok(results)
}

fn print_entries(entries: &[Entry]) {
    let mut current = "";
    for e in entries {
        if e.category != current {
            current = e.category;
            println!("\n{}", current.bold().underline());
        }
        println!(
            "  {:<30} {:<45} {}",
            e.tool.name.cyan(),
            ui::truncate(&e.tool.description, 45),
            e.tool.url.dimmed()
        );
    }
    println!("\n{} tools", entries.len());
}

fn print_results(results: &[health::CheckResult], down_only: bool) {
    for r in results {
        println!(
            "{} {:<30} {:<32} {:<26} {}",
            ui::badge(&r.status),
            ui::truncate(&r.entry.tool.name, 30),
            ui::truncate(r.entry.category, 32).dimmed(),
            ui::detail(&r.status),
            format!("{}ms", r.elapsed_ms).dimmed()
        );
    }
    if !down_only {
        let count =
            |f: fn(&health::Status) -> bool| results.iter().filter(|r| f(&r.status)).count();
        let active = count(|s| matches!(s, health::Status::Active { .. }));
        let guarded = count(|s| matches!(s, health::Status::Guarded { .. }));
        let down = count(health::Status::is_down);
        println!(
            "\n{} active · {} guarded · {} down",
            active.to_string().green().bold(),
            guarded.to_string().yellow().bold(),
            down.to_string().red().bold()
        );
    }
}

async fn open_tools(targets: Vec<Entry<'_>>, no_check: bool, http: &cli::HttpArgs) -> Result<()> {
    let to_open: Vec<Entry> = if no_check {
        targets
    } else {
        let results = run_checks(targets, http.concurrency.into(), http.timeout).await?;
        results
            .into_iter()
            .filter_map(|r| {
                if r.status.is_down() {
                    ui::warn(format!(
                        "skipping {} ({})",
                        r.entry.tool.name,
                        ui::detail(&r.status)
                    ));
                    None
                } else {
                    Some(r.entry)
                }
            })
            .collect()
    };
    for e in to_open {
        open::that_detached(&e.tool.url)
            .with_context(|| format!("opening {} in the default browser", e.tool.url))?;
        ui::success(format!("opened {} → {}", e.tool.name, e.tool.url.dimmed()));
    }
    Ok(())
}

async fn run_persona(cmd: PersonaCmd) -> Result<()> {
    match cmd {
        PersonaCmd::Start(args) => start_persona(args).await,
        PersonaCmd::Stop { name } => {
            let names = PersonaNames::new(&name)?;
            let manager = Manager::connect().await?;
            match manager.stop(&names).await? {
                0 => ui::warn(format!("persona '{name}' was not running")),
                _ => ui::success(format!("stopped persona '{name}' (profile data kept)")),
            }
            Ok(())
        }
        PersonaCmd::List => {
            let manager = Manager::connect().await?;
            let personas = manager.list().await?;
            if personas.is_empty() {
                println!("No personas yet. Create one with `ridge persona start <name>`.");
                return Ok(());
            }
            println!(
                "{:<20} {:<26} {:<26} {:<8} {}",
                "PERSONA".bold(),
                "BROWSER".bold(),
                "VPN".bold(),
                "DATA".bold(),
                "GUI".bold()
            );
            for (name, p) in personas {
                let gui = match PersonaNames::new(&name) {
                    Ok(n) if p.browser.as_deref().is_some_and(|s| s.starts_with("Up")) => manager
                        .gui_port(&n)
                        .await
                        .map(|port| format!("http://127.0.0.1:{port}"))
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                println!(
                    "{:<20} {:<26} {:<26} {:<8} {}",
                    name.cyan(),
                    p.browser.as_deref().unwrap_or("-"),
                    p.vpn.as_deref().unwrap_or("-"),
                    if p.has_volume {
                        "yes".green()
                    } else {
                        "no".red()
                    },
                    gui
                );
            }
            Ok(())
        }
        PersonaCmd::Clean { name, all } => {
            let manager = Manager::connect().await?;
            let removed = if all {
                manager.clean_all().await?
            } else {
                let name = name.expect("clap enforces name or --all");
                manager.clean(&PersonaNames::new(&name)?).await?
            };
            ui::success(format!(
                "removed {removed} container(s); persistent profile volumes were kept"
            ));
            Ok(())
        }
        PersonaCmd::Purge { name, yes } => {
            let names = PersonaNames::new(&name)?;
            if !yes
                && !ui::confirm_typed(
                    &format!(
                        "This permanently deletes volume {} (browser profile, history, bookmarks).",
                        names.volume.bold()
                    ),
                    &name,
                )
            {
                bail!("aborted; nothing was deleted");
            }
            let manager = Manager::connect().await?;
            if manager.purge(&names).await? {
                ui::success(format!("purged persona '{name}' and its data volume"));
            } else {
                ui::warn(format!(
                    "removed containers for '{name}'; no data volume existed"
                ));
            }
            Ok(())
        }
    }
}

async fn start_persona(args: StartArgs) -> Result<()> {
    let open_after = args.open;
    let opts = start_options(args)?;
    let manager = Manager::connect().await?;
    let vpn_desc = opts
        .vpn
        .as_ref()
        .map(|v| format!("VPN exit: {}", v.country))
        .unwrap_or_else(|| "isolated bridge network, no VPN".into());
    println!(
        "Starting persona {} ({vpn_desc})",
        opts.names.persona.cyan().bold()
    );
    manager.start(&opts).await?;

    let url = format!("http://127.0.0.1:{}", opts.host_port);
    ui::success(format!("persona '{}' is running", opts.names.persona));
    println!("  GUI:     {}", url.cyan());
    println!("  Profile: volume {}", opts.names.volume);
    println!("  Stop:    ridge persona stop {}", opts.names.persona);

    if open_after {
        open::that_detached(&url).context("opening the persona GUI")?;
    }
    Ok(())
}

/// Validates start arguments and turns them into Docker start options (shared by the CLI and the window).
pub(crate) fn start_options(args: StartArgs) -> Result<StartOptions> {
    let names = PersonaNames::new(&args.name)?;
    let vpn = args
        .vpn_country
        .map(|country| VpnConfig::new(country, args.vpn_provider, args.vpn_env_file.as_deref()))
        .transpose()?;

    let pc = persona::hostnet::pc_address();
    let folders = if args.bookmarks {
        bookmarks::ridge_folders(pc.as_deref(), persona::hostnet::router_address().as_deref())
    } else {
        Vec::new()
    };

    let host_port = match args.port {
        Some(p) => p,
        None => persona::find_free_port(3000).context("no free local port found in 3000-3099")?,
    };

    // a VPN's own firewall already keeps the persona off the PC and the home network
    let protections = args.protections.unwrap_or_default();
    let firewall_script = (vpn.is_none() && protections.blocks_anything()).then(|| {
        let pc: Vec<String> = pc.into_iter().collect();
        persona::firewall::rules_script(&protections, &pc)
    });

    Ok(StartOptions {
        names,
        browser_image: args.image,
        vpn_image: args.vpn_image,
        vpn,
        host_port,
        tz: args.tz,
        policy_archive: Some(bookmarks::policy_archive(&folders)?),
        pull: args.pull,
        firewall_script,
        gui_password: args.gui_password,
        protections,
    })
}
