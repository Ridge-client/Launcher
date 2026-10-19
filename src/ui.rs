//! Terminal presentation helpers: badges, progress bars and prompts.

use std::io::{BufRead, Write};
use std::time::Duration;

use colored::{ColoredString, Colorize};
use indicatif::{ProgressBar, ProgressStyle};

use crate::catalog::health::Status;

pub fn init() {
    #[cfg(windows)]
    {
        // Ridge is built as a window app, so on Windows it starts without a console.
        // When run from a terminal, print into that terminal.
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: plain Win32 call with no pointers; failure (no parent console) is harmless.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        let _ = colored::control::set_virtual_terminal(true);
    }
}

pub fn badge(status: &Status) -> ColoredString {
    match status {
        Status::Active { .. } => " ACTIVE  ".black().on_green().bold(),
        Status::Guarded { .. } => " GUARDED ".black().on_yellow().bold(),
        Status::Down { .. } => "  DOWN   ".white().on_red().bold(),
    }
}

pub fn detail(status: &Status) -> String {
    match status {
        Status::Active { code } => format!("HTTP {code}"),
        Status::Guarded { code } => format!("HTTP {code} (bot-protected)"),
        Status::Down { reason } => reason.clone(),
    }
}

pub fn progress_bar(len: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} {msg} [{bar:30.cyan/blue}] {pos}/{len} ({elapsed})",
        )
        .expect("valid template")
        .progress_chars("=> "),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").expect("valid template"));
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn success(msg: impl AsRef<str>) {
    println!("{} {}", "✔".green().bold(), msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    eprintln!("{} {}", "!".yellow().bold(), msg.as_ref());
}

/// Asks the user to type `expected` to confirm a destructive action.
pub fn confirm_typed(prompt: &str, expected: &str) -> bool {
    print!("{prompt} Type '{}' to confirm: ", expected.bold());
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok() && line.trim() == expected
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
