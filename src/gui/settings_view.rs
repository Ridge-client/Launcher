//! "Settings" page, laid out as the Cairn launcher's: the protections that keep every persona's
//! browser away from this PC, the home network and the other personas, and checks run from
//! inside each running persona to show what it can actually reach.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use eframe::egui;

use super::icons;
use super::persona_view::{self, ALL_PERSONAS};
use super::{
    caps, primary_button, secondary_button, Msg, RidgeApp, NAV_ACCENT, NAV_MUTED, NAV_TEXT,
};
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
    page: Page,
    search: String,
}

/// The page's tabs.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Protections,
    Checks,
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
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink(false)
        .show(ui, |ui| {
            // one centred column
            let width = ui.available_width().min(760.0);
            let side = ((ui.available_width() - width) / 2.0).max(0.0);
            ui.horizontal_top(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    column(app, ui);
                });
            });
        });
}

fn column(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.label(caps("Settings", 20.0).strong().color(NAV_TEXT));
    });
    ui.add_space(18.0);
    let search = ui.add(
        egui::TextEdit::singleline(&mut app.settings.search)
            .hint_text("Search settings")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(14, 10))
            .font(egui::FontId::proportional(16.0)),
    );
    if search.changed() && !app.settings.search.trim().is_empty() {
        app.settings.page = Page::Protections;
    }
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (page, label) in [(Page::Protections, "Protections"), (Page::Checks, "Checks")] {
            if sub_tab(ui, label, app.settings.page == page).clicked() {
                app.settings.page = page;
            }
        }
    });
    let r = ui.min_rect();
    ui.painter().hline(
        r.x_range(),
        ui.cursor().top() - ui.spacing().item_spacing.y / 2.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(26)),
    );
    ui.add_space(18.0);
    match app.settings.page {
        Page::Protections => protections_section(app, ui),
        Page::Checks => {
            let running = persona_view::running_saved(app);
            checks(app, ui, &running);
        }
    }
}

/// A tab of the page: its name, white with an amber line under it when open.
fn sub_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let font = egui::FontId::proportional(17.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), NAV_TEXT);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 40.0, 44.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        if active || response.hovered() {
            NAV_TEXT
        } else {
            NAV_MUTED
        },
    );
    if active {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 12.0, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 12.0, rect.bottom()),
            ),
            1.0,
            NAV_ACCENT,
        );
    }
    response
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
    let search = app.settings.search.trim().to_lowercase();
    let mut changed = false;
    let mut shown = 0;
    ui.label(
        egui::RichText::new("What every persona's browser is kept away from.")
            .size(15.0)
            .color(NAV_MUTED),
    );
    ui.add_space(14.0);
    for (i, (on, label, hint)) in [
        (
            &mut p.block_pc,
            "Block this PC",
            "File sharing, remote access and the other services of this computer.",
        ),
        (
            &mut p.block_lan,
            "Block my home network",
            "The router and every other device on your network.",
        ),
        (
            &mut p.block_personas,
            "Block other personas",
            "Personas can't reach each other, directly or through this PC.",
        ),
        (
            &mut p.password,
            "Password-protect the remote screen",
            "Other programs on this PC can't open a persona's browser; Ridge fills the password in.",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let text = format!("{label} {hint}").to_lowercase();
        if !search.is_empty() && !text.contains(&search) {
            continue;
        }
        shown += 1;
        changed |= switch(ui, on, mixed[i], label, hint);
        ui.add_space(14.0);
    }
    if shown == 0 {
        ui.label(
            egui::RichText::new("No setting matches.")
                .size(15.0)
                .color(NAV_MUTED),
        );
    }
    if changed {
        app.settings.draft = Some(p);
    }
    ui.add_space(10.0);
    apply_row(app, ui, &running, p);
}

/// The state of the switches, then Save & apply and Cancel: Save & apply saves them and
/// restarts the running personas that don't use them yet; Cancel forgets the changes.
fn apply_row(app: &mut RidgeApp, ui: &mut egui::Ui, running: &[String], target: Protections) {
    let idle = app.personas.busy.is_none() && !app.settings.busy();
    let outdated: Vec<String> = running
        .iter()
        .filter(|id| actual(app, id) != target)
        .cloned()
        .collect();
    let n = outdated.len();
    let plural = if n == 1 { "" } else { "s" };
    ui.vertical_centered(|ui| match &app.personas.busy {
        Some((busy_id, doing)) if busy_id == ALL_PERSONAS => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(doing);
            });
        }
        _ if n > 0 => {
            ui.colored_label(
                UNSAFE,
                format!("{n} running persona{plural} don't use these yet: Save & apply restarts them (browser profiles are kept)."),
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
    });
    if let Some((short, detail)) = app.personas.errors.get(ALL_PERSONAS) {
        ui.vertical_centered(|ui| {
            ui.colored_label(UNSAFE, icons::with(icons::ERROR, short))
                .on_hover_text(detail);
        });
    }
    ui.add_space(28.0);
    let (mut save, mut cancel) = (false, false);
    // the two buttons, centred
    let width = 220.0 + 12.0 + 140.0;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        save = ui
            .add_enabled(
                idle && (app.settings.draft.is_some() || n > 0),
                primary_button("Save & apply").min_size(egui::vec2(220.0, 44.0)),
            )
            .on_hover_text(
                "Saves these protections and restarts the running personas that don't use them yet",
            )
            .clicked();
        ui.add_space(12.0);
        cancel = ui
            .add_enabled(
                app.settings.draft.is_some(),
                secondary_button("Cancel").min_size(egui::vec2(140.0, 44.0)),
            )
            .on_hover_text("Forget the changes")
            .clicked();
    });
    if save {
        app.settings.draft = None;
        app.personas.protections = target;
        app.personas.errors.remove(ALL_PERSONAS);
        if !outdated.is_empty() {
            app.settings.results.clear();
            persona_view::apply_protections(app, &outdated);
        }
    }
    if cancel {
        app.settings.draft = None;
    }
}

/// Run checks: a test persona made with the saved protections, and every running persona.
fn checks(app: &mut RidgeApp, ui: &mut egui::Ui, running: &[String]) {
    let idle = app.personas.busy.is_none() && !app.settings.busy();
    ui.label(
        egui::RichText::new(
            "Proof that the protections hold: Ridge tries to reach your PC, your router and the \
             other personas from a test persona made with the saved protections, and from inside \
             each running persona.",
        )
        .size(15.0)
        .color(NAV_MUTED),
    );
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(idle, primary_button("Run checks"))
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

/// A switch, as the Cairn launcher's: a box and the setting in spaced capitals, what it does
/// under it. `mixed` when running personas differ. Returns whether it was clicked.
fn switch(ui: &mut egui::Ui, on: &mut bool, mixed: bool, label: &str, hint: &str) -> bool {
    let clicked = ui
        .add(
            egui::Checkbox::new(on, caps(label, 13.5).color(NAV_MUTED).monospace())
                .indeterminate(mixed),
        )
        .on_hover_text_at_pointer(if mixed {
            "On for some running personas, off for others"
        } else {
            hint
        })
        .changed();
    ui.label(egui::RichText::new(hint).size(15.0).color(NAV_MUTED));
    clicked
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
