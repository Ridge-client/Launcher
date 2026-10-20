//! "Settings" tab: the protections that keep every persona's browser away from this PC, the home
//! network and the other personas, and checks run from inside each running persona to show what
//! it can actually reach.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use eframe::egui;

use super::icons;
use super::persona_view::{self, ALL_PERSONAS};
use super::{Msg, RidgeApp};
use crate::catalog::health;
use crate::persona::docker::Manager;
use crate::persona::firewall::{self, Check, OtherPersona, Protections};
use crate::persona::{hostnet, PersonaNames};

const SAFE: egui::Color32 = egui::Color32::from_rgb(46, 160, 67);
const UNSAFE: egui::Color32 = egui::Color32::from_rgb(218, 54, 51);

#[derive(Default)]
pub(super) struct State {
    /// Personas whose checks are running.
    checking: BTreeSet<String>,
    /// Per persona: the last checks, or why they could not run.
    results: BTreeMap<String, Result<Vec<Check>, String>>,
    /// Switches changed but not applied yet.
    draft: Option<Protections>,
}

impl State {
    pub(super) fn busy(&self) -> bool {
        !self.checking.is_empty()
    }

    pub(super) fn checks_done(&mut self, id: String, result: Result<Vec<Check>, String>) {
        self.checking.remove(&id);
        self.results.insert(id, result);
    }
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading("Settings");
    ui.add_space(10.0);
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink(false)
        .show(ui, |ui| {
            protections_section(app, ui);
        });
}

/// Results key of the test persona the checks make.
const TEST_PERSONA: &str = "";

/// The test-persona check, as the persona start pop-up shows it.
pub(super) enum ProtectionCheck {
    Running,
    /// Every line was safe; how many there were.
    Passed(usize),
    /// The lines that were not safe.
    Failed(Vec<String>),
    /// The check could not run.
    Error(String),
}

/// Runs the test-persona check (with the applied protections) unless it is running already.
pub(super) fn start_protection_check(app: &mut RidgeApp) {
    if !app.settings.checking.contains(TEST_PERSONA) {
        app.settings.results.remove(TEST_PERSONA);
        run_test_checks(app);
    }
}

pub(super) fn protection_check(app: &RidgeApp) -> ProtectionCheck {
    match app.settings.results.get(TEST_PERSONA) {
        _ if app.settings.checking.contains(TEST_PERSONA) => ProtectionCheck::Running,
        None => ProtectionCheck::Running,
        Some(Err(e)) => ProtectionCheck::Error(e.clone()),
        Some(Ok(checks)) => {
            let failed: Vec<String> = checks
                .iter()
                .filter(|c| !c.safe)
                .map(|c| format!("{}: {} ({})", c.group, c.label, c.seen))
                .collect();
            if failed.is_empty() {
                ProtectionCheck::Passed(checks.len())
            } else {
                ProtectionCheck::Failed(failed)
            }
        }
    }
}

/// What the running personas really use, per switch: the value when they all agree, none when
/// they differ. With no persona running, the saved protections.
fn in_effect(app: &RidgeApp, running: &[String]) -> [Option<bool>; 4] {
    let flags = |p: &Protections| [p.block_pc, p.block_lan, p.block_personas, p.password];
    if running.is_empty() {
        return flags(&app.personas.protections).map(Some);
    }
    let actual: Vec<[bool; 4]> = running.iter().map(|id| flags(&actual(app, id))).collect();
    std::array::from_fn(|i| {
        let first = actual[0][i];
        actual.iter().all(|a| a[i] == first).then_some(first)
    })
}

/// The protections persona `id` was started with.
fn actual(app: &RidgeApp, id: &str) -> Protections {
    app.personas
        .rows
        .get(id)
        .and_then(|r| r.protections)
        .unwrap_or(Protections::NONE)
}

fn protections_section(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let running = persona_view::running_saved(app);
    let effect = in_effect(app, &running);
    let saved = app.personas.protections;
    // the switches: the unapplied changes, or what is really in effect (the saved value under a
    // mixed box)
    let mut p = app.settings.draft.unwrap_or(Protections {
        block_pc: effect[0].unwrap_or(saved.block_pc),
        block_lan: effect[1].unwrap_or(saved.block_lan),
        block_personas: effect[2].unwrap_or(saved.block_personas),
        password: effect[3].unwrap_or(saved.password),
    });
    let mixed = |i: usize| app.settings.draft.is_none() && effect[i].is_none();
    let mixed = [mixed(0), mixed(1), mixed(2), mixed(3)];
    let mut changed = false;
    section(ui, "Protections", |ui| {
        ui.weak("What every persona's browser is kept away from.");
        ui.add_space(4.0);
        for (i, (on, label, hint)) in [
            (
                &mut p.block_pc,
                "Block this PC",
                "File sharing, remote access and the other services of this computer",
            ),
            (
                &mut p.block_lan,
                "Block my home network",
                "The router and every other device on your network",
            ),
            (
                &mut p.block_personas,
                "Block other personas",
                "Personas can't reach each other, directly or through this PC",
            ),
            (
                &mut p.password,
                "Password-protect the remote screen",
                "Other programs on this PC can't open a persona's browser; Ridge fills the password in",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            changed |= switch(ui, on, mixed[i], label, hint);
        }
        if changed {
            app.settings.draft = Some(p);
        }

        ui.add_space(8.0);
        apply_row(app, ui, &running, p);
        ui.add_space(4.0);
        checks(app, ui, &running);
    });
}

/// Apply: saves the switches and restarts the running personas that don't use them yet.
fn apply_row(app: &mut RidgeApp, ui: &mut egui::Ui, running: &[String], target: Protections) {
    let idle = app.personas.busy.is_none() && !app.settings.busy();
    let outdated: Vec<String> = running
        .iter()
        .filter(|id| actual(app, id) != target)
        .cloned()
        .collect();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                idle,
                egui::Button::new(egui::RichText::new(icons::with(icons::SAVE, "Apply")).strong()),
            )
            .on_hover_text("Saves these protections and restarts the running personas that don't use them yet")
            .clicked()
        {
            app.settings.draft = None;
            app.personas.protections = target;
            app.personas.errors.remove(ALL_PERSONAS);
            if !outdated.is_empty() {
                app.settings.results.clear();
                persona_view::apply_protections(app, &outdated);
            }
        }
        let n = outdated.len();
        let plural = if n == 1 { "" } else { "s" };
        match &app.personas.busy {
            Some((busy_id, doing)) if busy_id == ALL_PERSONAS => {
                ui.spinner();
                ui.weak(doing);
            }
            _ if n > 0 => {
                ui.colored_label(
                    UNSAFE,
                    format!("{n} running persona{plural} don't use these yet: Apply restarts them (browser profiles are kept)."),
                );
            }
            _ if app.settings.draft.is_some() => {
                ui.weak("Not applied yet.");
            }
            _ if running.is_empty() => {
                ui.weak("Personas use these each time they start.");
            }
            _ => {
                ui.weak("In effect on every running persona.");
            }
        }
    });
    if let Some((short, detail)) = app.personas.errors.get(ALL_PERSONAS) {
        ui.colored_label(UNSAFE, icons::with(icons::ERROR, short))
            .on_hover_text(detail);
    }
}

/// Run checks: a test persona made with the saved protections, and every running persona.
fn checks(app: &mut RidgeApp, ui: &mut egui::Ui, running: &[String]) {
    let idle = app.personas.busy.is_none() && !app.settings.busy();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                idle,
                egui::Button::new(icons::with(icons::SHIELD, "Run checks")),
            )
            .on_hover_text(
                "Tries to reach your PC, router and personas from a test persona made with the \
                 applied protections, and from inside each running persona",
            )
            .clicked()
        {
            app.settings.results.clear();
            run_test_checks(app);
            for id in running {
                run_checks(app, id);
            }
        }
        if app.settings.busy() {
            ui.spinner();
            ui.weak("Checking…");
        }
    });
    for (id, result) in &app.settings.results {
        ui.add_space(10.0);
        let name = if id == TEST_PERSONA {
            "Test persona (applied protections)".to_string()
        } else {
            persona_view::display_name(app, id)
        };
        ui.label(egui::RichText::new(name).strong().size(15.0));
        match result {
            Ok(checks) => check_lines(ui, checks),
            Err(err) => {
                ui.colored_label(UNSAFE, icons::with(icons::ERROR, "The checks couldn't run"))
                    .on_hover_text(err);
            }
        }
    }
}

/// One persona's results: a summary, then the lines needing attention; the safe ones fold away.
fn check_lines(ui: &mut egui::Ui, checks: &[Check]) {
    let open: Vec<&Check> = checks.iter().filter(|c| !c.safe).collect();
    if open.is_empty() {
        ui.colored_label(
            SAFE,
            icons::with(
                icons::CHECK,
                &format!("Sealed off: all {} checks passed", checks.len()),
            ),
        );
    } else {
        ui.colored_label(
            UNSAFE,
            icons::with(
                icons::WARNING,
                &format!("{} of {} checks need attention", open.len(), checks.len()),
            ),
        );
        for c in open {
            check_line(ui, c);
        }
    }
    ui.collapsing("Every check", |ui| {
        for c in checks {
            check_line(ui, c);
        }
    });
}

fn check_line(ui: &mut egui::Ui, c: &Check) {
    ui.horizontal(|ui| {
        let (icon, color) = if c.safe {
            (icons::CHECK, SAFE)
        } else {
            (icons::ERROR, UNSAFE)
        };
        ui.colored_label(color, icon);
        ui.label(format!("{}: {}", c.group, c.label));
        ui.weak(format!("·  {}", c.seen));
    });
}

pub(super) fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).strong().size(16.0));
            ui.add_space(4.0);
            body(ui);
        });
}

/// A switch; `mixed` when running personas differ. Returns whether it was clicked.
fn switch(ui: &mut egui::Ui, on: &mut bool, mixed: bool, label: &str, hint: &str) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui
            .add(egui::Checkbox::new(on, label).indeterminate(mixed))
            .on_hover_text_at_pointer(if mixed {
                "On for some running personas, off for others"
            } else {
                hint
            })
            .changed();
        ui.weak(hint);
        clicked
    })
    .inner
}

/// The running personas other than `id`, to try reaching them.
fn others(app: &RidgeApp, id: &str) -> Vec<String> {
    app.personas
        .rows
        .iter()
        .filter(|(other, row)| other.as_str() != id && row.running())
        .map(|(other, _)| other.clone())
        .collect()
}

/// The checks script for a persona that should not reach `others`.
async fn script_for(
    manager: &Manager,
    others: Vec<String>,
    in_browser: bool,
) -> anyhow::Result<String> {
    let mut other_personas = Vec::new();
    for other in others {
        let Ok(n) = PersonaNames::new(&other) else {
            continue;
        };
        other_personas.push(OtherPersona {
            ip: manager.persona_ip(&n).await,
            port: manager.gui_port(&n).await,
            name: other,
        });
    }
    let (pc, router) =
        tokio::task::spawn_blocking(|| (hostnet::pc_address(), hostnet::router_address())).await?;
    Ok(firewall::checks_script(
        pc.as_deref(),
        router.as_deref(),
        &other_personas,
        in_browser,
    ))
}

/// Makes a test persona with the applied protections, checks from it, and removes it.
fn run_test_checks(app: &mut RidgeApp) {
    let others = others(app, TEST_PERSONA);
    let protections = app.personas.protections;
    app.settings.checking.insert(TEST_PERSONA.to_string());
    app.spawn(async move {
        let result = async {
            let manager = Manager::connect().await?;
            let script = script_for(&manager, others, false).await?;
            let pc: Vec<String> = hostnet::pc_address().into_iter().collect();
            let rules = firewall::rules_script(&protections, &pc);
            anyhow::Ok(firewall::parse_checks(
                &manager.run_in_test_persona(&rules, &script).await?,
            ))
        }
        .await;
        Msg::SettingsChecks {
            id: TEST_PERSONA.to_string(),
            result: result.map_err(|e| format!("{e:#}")),
        }
    });
}

/// Runs the checks inside persona `id` in the background.
fn run_checks(app: &mut RidgeApp, id: &str) {
    let Ok(names) = PersonaNames::new(id) else {
        return;
    };
    let others = others(app, id);
    let gui_url = app.personas.rows.get(id).and_then(|r| r.gui_url.clone());
    app.settings.checking.insert(id.to_string());
    let id = id.to_string();
    app.spawn(async move {
        let result = async {
            let manager = Manager::connect().await?;
            let script = script_for(&manager, others, true).await?;
            let mut checks =
                firewall::parse_checks(&manager.run_in_browser(&names, &script).await?);
            if let Some(url) = gui_url {
                checks.push(screen_check(&url).await);
            }
            anyhow::Ok(checks)
        }
        .await;
        Msg::SettingsChecks {
            id,
            result: result.map_err(|e| format!("{e:#}")),
        }
    });
}

/// From this PC: does the persona's remote screen open without its password?
async fn screen_check(url: &str) -> Check {
    let status = match health::client(Duration::from_secs(5)) {
        Ok(client) => client
            .get(url)
            .send()
            .await
            .map(|r| r.status().as_u16())
            .ok(),
        Err(_) => None,
    };
    let (seen, safe) = match status {
        Some(401) => ("asks for the password", true),
        Some(200..=399) => ("opens without a password", false),
        _ => ("did not answer", true),
    };
    Check {
        group: "Remote screen".into(),
        label: "Opening it without Ridge".into(),
        seen: seen.into(),
        safe,
    }
}
