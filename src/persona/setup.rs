//! Getting Docker running on Windows when the daemon cannot be reached: start Docker Desktop if it
//! is installed, otherwise download its installer.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

/// What the "Docker is not available" banner offers.
pub enum DockerFix {
    /// Docker Desktop is installed but not running.
    Start(PathBuf),
    /// Docker Desktop's official installer.
    Install {
        label: &'static str,
        url: &'static str,
    },
}

pub fn docker_fix() -> DockerFix {
    match docker_desktop_path() {
        Some(path) => DockerFix::Start(path),
        None => DockerFix::Install {
            label: "Install Docker Desktop",
            url: install_url(),
        },
    }
}

/// Docker Desktop installed machine-wide, or per user (where newer versions install by default).
fn docker_desktop_path() -> Option<PathBuf> {
    let dir = |var: &str| std::env::var_os(var).map(PathBuf::from);
    [
        dir("ProgramFiles").map(|p| p.join(r"Docker\Docker\Docker Desktop.exe")),
        dir("LOCALAPPDATA").map(|p| p.join(r"Programs\DockerDesktop\Docker Desktop.exe")),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.exists())
}

pub fn start_docker_desktop(path: &PathBuf) -> Result<()> {
    Command::new(path)
        .spawn()
        .with_context(|| format!("starting {}", path.display()))?;
    Ok(())
}

fn install_url() -> &'static str {
    // an x64 build may run emulated on ARM: ask Windows for the real CPU
    let arch = std::env::var("PROCESSOR_ARCHITEW6432")
        .or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE"))
        .unwrap_or_default();
    if arch.eq_ignore_ascii_case("ARM64") {
        "https://desktop.docker.com/win/main/arm64/Docker%20Desktop%20Installer.exe"
    } else {
        "https://desktop.docker.com/win/main/amd64/Docker%20Desktop%20Installer.exe"
    }
}
