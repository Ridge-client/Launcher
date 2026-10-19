//! Command-line interface definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ridge",
    version,
    about = "OSINT tool catalog, health checker and isolated Docker persona manager",
    propagate_version = true
)]
pub struct Cli {
    /// Load the OSINT catalog from this JSON file instead of the embedded one
    #[arg(long, global = true, env = "RIDGE_CATALOG", value_name = "FILE")]
    pub catalog: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Browse, health-check and open OSINT tools
    #[command(subcommand)]
    Catalog(CatalogCmd),

    /// Manage isolated sock-puppet browser containers
    #[command(subcommand)]
    Persona(PersonaCmd),
}

#[derive(Debug, Subcommand)]
pub enum CatalogCmd {
    /// List catalog categories and tool counts
    Categories,

    /// List tools, optionally filtered
    List {
        #[command(flatten)]
        filter: FilterArgs,

        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },

    /// Concurrently check whether each tool's website is reachable
    Check {
        #[command(flatten)]
        filter: FilterArgs,

        #[command(flatten)]
        http: HttpArgs,

        /// Only print tools that are down
        #[arg(long)]
        down_only: bool,

        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },

    /// Open tools in the system's default browser (down sites are skipped)
    Open {
        /// Tool names (case-insensitive, exact match)
        #[arg(value_name = "NAME", required_unless_present = "category")]
        names: Vec<String>,

        /// Open every tool in a category
        #[arg(short, long)]
        category: Option<String>,

        /// Skip the health check and open regardless
        #[arg(long)]
        no_check: bool,

        /// Allow opening more than 10 tabs at once
        #[arg(short, long)]
        yes: bool,

        #[command(flatten)]
        http: HttpArgs,
    },
}

#[derive(Debug, Args)]
pub struct FilterArgs {
    /// Only tools whose category contains this text (case-insensitive)
    #[arg(short, long)]
    pub category: Option<String>,

    /// Only tools whose name or description contains this text
    #[arg(short, long)]
    pub search: Option<String>,
}

#[derive(Debug, Args)]
pub struct HttpArgs {
    /// Maximum number of simultaneous requests
    #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u16).range(1..=256))]
    pub concurrency: u16,

    /// Per-request timeout in seconds
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=120))]
    pub timeout: u64,
}

#[derive(Debug, Subcommand)]
pub enum PersonaCmd {
    /// Start (or restart) a persona's browser container
    Start(StartArgs),

    /// Stop a persona's running containers (containers and data are kept)
    Stop {
        /// Persona name
        name: String,
    },

    /// List personas, their containers and storage
    List,

    /// Remove persona containers and networks (persistent data is kept)
    Clean {
        /// Persona name
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        name: Option<String>,

        /// Clean every Ridge-managed persona
        #[arg(long)]
        all: bool,
    },

    /// Permanently delete a persona, INCLUDING its browser profile volume
    Purge {
        /// Persona name
        name: String,

        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
pub struct StartArgs {
    /// Persona name (lowercase letters, digits, '-' and '_')
    pub name: String,

    /// Route all browser traffic through a VPN sidecar exiting in this country
    /// (gluetun country name, e.g. "Netherlands")
    #[arg(long, value_name = "COUNTRY")]
    pub vpn_country: Option<String>,

    /// VPN provider understood by gluetun (e.g. mullvad, protonvpn, nordvpn)
    #[arg(long, env = "RIDGE_VPN_PROVIDER", requires = "vpn_country")]
    pub vpn_provider: Option<String>,

    /// KEY=VALUE file with VPN credentials/settings passed to the gluetun sidecar
    #[arg(
        long,
        env = "RIDGE_VPN_ENV_FILE",
        value_name = "FILE",
        requires = "vpn_country"
    )]
    pub vpn_env_file: Option<PathBuf>,

    /// Local port for the browser GUI (bound to 127.0.0.1); defaults to the first free port from 3000
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Container timezone; match it to the VPN exit to keep the persona consistent
    #[arg(long, default_value = "Etc/UTC")]
    pub tz: String,

    /// Add Ridge's bookmarks to the persona's browser: integrity tests and social networks
    #[arg(long)]
    pub bookmarks: bool,

    /// Browser image (must serve a GUI on container port 3000 and keep its profile in /config)
    #[arg(long, default_value = crate::persona::DEFAULT_BROWSER_IMAGE)]
    pub image: String,

    /// VPN sidecar image
    #[arg(long, default_value = crate::persona::DEFAULT_VPN_IMAGE)]
    pub vpn_image: String,

    /// Always pull the latest images before starting
    #[arg(long)]
    pub pull: bool,

    /// Open the browser GUI in the local default browser once started
    #[arg(long)]
    pub open: bool,

    /// What the persona is kept away from; the window uses its Settings tab, the command line
    /// uses every protection.
    #[arg(skip)]
    pub protections: Option<crate::persona::firewall::Protections>,

    /// Password of the persona's remote screen (set by the window).
    #[arg(skip)]
    pub gui_password: Option<String>,
}
