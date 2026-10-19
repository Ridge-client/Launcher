//! "Personas" tab: saved personas, and starting, stopping, opening and deleting their isolated
//! browsers. A persona is created and saved first (its identity sheet and browser settings), then
//! started from the list.

mod builder;
mod notes;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use anyhow::Context;
use eframe::egui;
use serde::{Deserialize, Serialize};

use super::browser::Browser;
use super::icons;
use super::settings_view::{self, ProtectionCheck};
use super::{Msg, PersonaRow, RidgeApp};
use crate::catalog::health;
use crate::cli::StartArgs;
use crate::persona::docker::{Manager, GUI_USER};
use crate::persona::firewall::Protections;
use crate::persona::profile::{is_filled, Group, Part, Profile, Record};
use crate::persona::setup::{self, DockerFix};
use crate::persona::{PersonaNames, DEFAULT_BROWSER_IMAGE, DEFAULT_VPN_IMAGE};

/// Width of the Profile panel, and of the persona form in the same place.
const PROFILE_WIDTH: f32 = 560.0;
const PERSONAS_KEY: &str = "personas";
const PROTECTIONS_KEY: &str = "protections";
/// `busy` id of an action on every running persona (Settings' Apply).
pub(super) const ALL_PERSONAS: &str = "";
/// Identity sheets saved by earlier versions (profile only), read once and converted.
const OLD_PROFILES_KEY: &str = "persona_profiles";
const DOCKER_RETRY_EVERY: Duration = Duration::from_secs(3);
const DOCKER_START_TIMEOUT: Duration = Duration::from_secs(180);
/// How long a started persona's browser may take to answer before Ridge opens it anyway.
const BROWSER_READY_TIMEOUT: Duration = Duration::from_secs(120);

/// A saved persona: its identity sheet and how its browser starts.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    profile: Profile,
    tz: String,
    bookmarks: bool,
    open_after: bool,
    /// When it was last started (unix seconds).
    last_started: Option<u64>,
    /// Password of its remote screen, made by Ridge.
    password: String,
    /// Its notes and attached files (under its Profile).
    notes: notes::Notes,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            profile: Profile::default(),
            tz: "Etc/UTC".into(),
            bookmarks: true,
            open_after: true,
            last_started: None,
            password: String::new(),
            notes: notes::Notes::default(),
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) rows: BTreeMap<String, PersonaRow>,
    pub(super) docker_error: Option<String>,
    pub(super) loading: bool,
    /// The persona an action is running for, and what its card says meanwhile ("Starting…").
    pub(super) busy: Option<(String, String)>,
    /// Per persona: what went wrong last (short, shown on its card) and the full error (on hover).
    pub(super) errors: HashMap<String, (String, String)>,
    /// Why Docker Desktop could not be started, shown in the Docker banner.
    docker_note: Option<String>,
    /// The persona being created or edited.
    form: Saved,
    /// Persona waiting for delete confirmation, and what the user typed.
    confirm_delete: Option<(String, String)>,
    /// Persona whose containers wait for "Remove containers" confirmation.
    confirm_remove: Option<String>,
    /// Persona about to start, and its pre-launch checklist.
    checklist: Option<(String, Checklist)>,
    /// Set when Ridge started Docker Desktop: reconnect on its own until Docker answers.
    docker_starting: Option<Instant>,
    last_retry: Option<Instant>,
    /// The "Create persona" screen is open; `editing` is the persona it edits, if any.
    creating: bool,
    editing: Option<String>,
    /// Built-in browser of that screen, and the generator site it shows.
    pub(super) browser: Browser,
    generator: Option<usize>,
    /// Saved personas by id (kept between runs).
    saved: BTreeMap<String, Saved>,
    /// Persona whose attachments pop-up is open.
    attachments_open: Option<String>,
    /// Persona whose identity sheet the right-hand panel shows.
    viewing: Option<String>,
    /// Loaded profile-picture thumbnails by file path (None: could not be read).
    photos: HashMap<String, Option<egui::TextureHandle>>,
    /// The browser image: downloading (progress 0..1) or ready, so starting a persona is quick.
    pub(super) image_progress: Option<f32>,
    pub(super) image_ready: bool,
    /// What every persona's browser is kept away from (Settings tab).
    pub(super) protections: Protections,
}

/// Ticked before a persona starts: its traffic leaves through this PC's own connection.
#[derive(Default)]
struct Checklist {
    vpn_on: bool,
    timezone_matches: bool,
    /// The Settings protections were checked from a test persona (started when the pop-up opens).
    checks_started: bool,
}

impl State {
    pub(super) fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut saved: BTreeMap<String, Saved> = storage
            .and_then(|s| eframe::get_value(s, PERSONAS_KEY))
            .unwrap_or_default();
        if saved.is_empty() {
            let old: HashMap<String, Profile> = storage
                .and_then(|s| eframe::get_value(s, OLD_PROFILES_KEY))
                .unwrap_or_default();
            saved.extend(old.into_iter().map(|(id, profile)| {
                (
                    id,
                    Saved {
                        profile,
                        ..Saved::default()
                    },
                )
            }));
        }
        // sheets saved before records existed: their fields move into records
        for persona in saved.values_mut() {
            persona.profile.migrate();
        }
        let protections = storage
            .and_then(|s| eframe::get_value(s, PROTECTIONS_KEY))
            .unwrap_or_default();
        Self {
            saved,
            protections,
            ..Self::default()
        }
    }

    pub(super) fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PERSONAS_KEY, &self.saved);
        eframe::set_value(storage, PROTECTIONS_KEY, &self.protections);
    }
}

/// Reloads the persona list from Docker in the background.
pub(super) fn refresh(app: &mut RidgeApp) {
    app.personas.loading = true;
    app.spawn(async {
        let result = async {
            let manager = Manager::connect().await?;
            let mut rows = BTreeMap::new();
            for (name, p) in manager.list().await? {
                let mut row = PersonaRow {
                    browser: p.browser,
                    gui_url: None,
                    protections: p.protections,
                };
                if row.running() {
                    if let Ok(names) = PersonaNames::new(&name) {
                        row.gui_url = manager
                            .gui_port(&names)
                            .await
                            .map(|port| format!("http://127.0.0.1:{port}"));
                    }
                }
                rows.insert(name, row);
            }
            anyhow::Ok(rows)
        }
        .await;
        Msg::Personas(result.map_err(|e| format!("{e:#}")))
    });
}

/// Downloads the browser image in the background (once), so starting a persona does not wait
/// for it. Called when a persona is saved, and when Docker answers while personas are saved.
pub(super) fn prepare_image(app: &mut RidgeApp) {
    let p = &mut app.personas;
    if p.image_ready || p.image_progress.is_some() || p.docker_error.is_some() {
        return;
    }
    p.image_progress = Some(0.0);
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.spawn(async move {
        let result = async {
            Manager::connect()
                .await?
                .prepare_image(DEFAULT_BROWSER_IMAGE, |progress| {
                    let _ = tx.send(Msg::ImageProgress(progress));
                    ctx.request_repaint();
                })
                .await
        }
        .await;
        Msg::ImageReady(result.map_err(|e| format!("{e:#}")))
    });
}

/// The persona's full name, or its id when the sheet has none.
pub(super) fn display_name(app: &RidgeApp, id: &str) -> String {
    app.personas
        .saved
        .get(id)
        .map(|s| s.profile.full_name())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.to_string())
}

/// Whether there are saved personas whose start would need the browser image.
pub(super) fn has_saved(app: &RidgeApp) -> bool {
    !app.personas.saved.is_empty()
}

/// Runs one action for persona `id` in the background. Its card says `doing` meanwhile, and
/// `failed` (with the full error on hover) if it goes wrong.
fn act<F>(app: &mut RidgeApp, id: &str, doing: &str, failed: &'static str, action: F)
where
    F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    app.personas.busy = Some((id.to_string(), doing.to_string()));
    app.personas.errors.remove(id);
    let id = id.to_string();
    app.spawn(async move {
        let error = action
            .await
            .err()
            .map(|e| (failed.to_string(), format!("{e:#}")));
        Msg::Done { id, error }
    });
}

/// While Docker Desktop boots: try to connect every few seconds, give up after a few minutes.
fn poll_docker_start(app: &mut RidgeApp) {
    let Some(started) = app.personas.docker_starting else {
        return;
    };
    if app.personas.docker_error.is_none() || started.elapsed() > DOCKER_START_TIMEOUT {
        app.personas.docker_starting = None;
        return;
    }
    let due = app
        .personas
        .last_retry
        .is_none_or(|t| t.elapsed() > DOCKER_RETRY_EVERY);
    if due && !app.personas.loading {
        app.personas.last_retry = Some(Instant::now());
        refresh(app);
    }
    app.ctx.request_repaint_after(DOCKER_RETRY_EVERY);
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui, frame: &eframe::Frame) {
    poll_docker_start(app);
    if app.personas.creating {
        builder::show(app, ui, frame);
        return;
    }
    app.personas.browser.hide();
    let idle = app.personas.busy.is_none();

    // the selected persona's identity sheet: on the right, the full height of the tab
    // dialogs open at the start of the frame: their clicks never deselect the persona
    let dialog_open = app.personas.checklist.is_some()
        || app.personas.confirm_remove.is_some()
        || app.personas.confirm_delete.is_some()
        || app.personas.attachments_open.is_some();

    let panel = egui::Panel::right("persona_profile")
        .resizable(false)
        .exact_size(PROFILE_WIDTH)
        .show(ui, |ui| profile_panel(app, ui));

    // the list keeps the same gap before the Profile panel as on its left
    let mut card_rects = Vec::new();
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 0,
            right: 10,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.add_space(8.0);
            if let Some(err) = app.personas.docker_error.clone() {
                docker_banner(app, ui, &err);
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                ui.heading("Your personas");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            idle,
                            egui::Button::new(
                                egui::RichText::new(icons::with(
                                    icons::NEW_PERSONA,
                                    "Create persona",
                                ))
                                .strong(),
                            ),
                        )
                        .on_hover_text("Build an identity, save it, then start it from the list")
                        .clicked()
                    {
                        open_builder(app, None);
                    }
                });
            });
            ui.add_space(6.0);

            if app.personas.saved.is_empty() && app.personas.rows.is_empty() {
                ui.weak("No personas yet. Press Create persona to build one.");
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("persona_list")
                    .auto_shrink(false)
                    .show(ui, |ui| card_rects = persona_cards(app, ui, idle));
            }
        });

    // a click anywhere but a persona card or the Profile panel clears the selection
    let clicked_at = ui.input(|i| {
        i.pointer
            .primary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if let Some(pos) = clicked_at {
        let on_card = card_rects.iter().any(|r| r.contains(pos));
        if !dialog_open && !on_card && !panel.response.rect.contains(pos) {
            app.personas.viewing = None;
        }
    }

    start_checklist(app, ui);
    remove_dialog(app, ui);
    delete_dialog(app, ui);
    notes::attachments_dialog(app, ui);
}

/// Opens the "Create persona" screen, empty or with a saved persona to edit.
fn open_builder(app: &mut RidgeApp, edit: Option<String>) {
    let p = &mut app.personas;
    p.form = edit
        .as_ref()
        .and_then(|id| p.saved.get(id).cloned())
        .unwrap_or_default();
    p.editing = edit;
    p.generator = None;
    p.creating = true;
}

/// Saves the persona on the "Create persona" screen and returns to the list.
fn save_persona(app: &mut RidgeApp, id: String) {
    let mut saved = app.personas.form.clone();
    if let Err(e) = keep_photo(&mut saved.profile, &id) {
        app.personas.errors.insert(
            id.clone(),
            ("Profile picture not saved".into(), format!("{e:#}")),
        );
    }
    app.personas.saved.insert(id.clone(), saved);
    app.personas.viewing = Some(id);
    app.personas.creating = false;
    app.personas.editing = None;
    prepare_image(app);
}

fn docker_banner(app: &mut RidgeApp, ui: &mut egui::Ui, err: &str) {
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(70, 30, 30))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(icons::with(icons::WARNING, "Docker is not available."))
                        .strong(),
                );
                match setup::docker_fix() {
                    DockerFix::Start(path) => {
                        ui.label("Docker Desktop is installed but not running.");
                        if app.personas.docker_starting.is_some() {
                            ui.spinner();
                            ui.label(
                                "Starting Docker Desktop… Ridge connects as soon as it is up.",
                            );
                        } else if ui
                            .button(icons::with(icons::START, "Start Docker Desktop"))
                            .clicked()
                        {
                            match setup::start_docker_desktop(&path) {
                                Ok(()) => app.personas.docker_starting = Some(Instant::now()),
                                Err(e) => app.personas.docker_note = Some(format!("{e:#}")),
                            }
                        }
                    }
                    DockerFix::Install { label, url } => {
                        ui.label("Personas need Docker. Install it, start it, then retry.");
                        if ui
                            .button(icons::with(icons::DOWNLOAD, label))
                            .on_hover_text(url)
                            .clicked()
                        {
                            let _ = open::that_detached(url);
                        }
                    }
                }
                if ui.button(icons::with(icons::RETRY, "Retry")).clicked() {
                    refresh(app);
                }
            });
            ui.weak(err);
            if let Some(note) = &app.personas.docker_note {
                ui.colored_label(egui::Color32::from_rgb(218, 54, 51), note);
            }
        });
}

/// "3 h ago" for a unix time.
fn ago(secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match now.saturating_sub(secs) {
        0..=59 => "just now".into(),
        d @ 60..=3599 => format!("{} min ago", d / 60),
        d @ 3600..=86_399 => format!("{} h ago", d / 3600),
        d => format!("{} days ago", d / 86_400),
    }
}

/// The card's second line: what is happening to the persona, or its state and last launch,
/// plus a short note if its last action failed (the full error on hover).
fn card_status(app: &RidgeApp, ui: &mut egui::Ui, id: &str, status: &str, last: &str) {
    ui.horizontal(|ui| {
        match &app.personas.busy {
            // Settings' Apply restarts every running persona
            Some((busy_id, doing))
                if busy_id == id || (busy_id == ALL_PERSONAS && status == "Running") =>
            {
                ui.spinner();
                ui.weak(doing);
            }
            _ => {
                ui.weak(format!("{status}  ·  Last launched {last}"));
            }
        }
        if let Some((short, detail)) = app.personas.errors.get(id) {
            ui.colored_label(egui::Color32::from_rgb(218, 54, 51), format!("·  {short}"))
                .on_hover_text(detail);
        }
    });
}

/// One full-width card per persona; clicking a card shows its identity on the right.
/// Returns the cards' rectangles, so a click elsewhere can clear the selection.
fn persona_cards(app: &mut RidgeApp, ui: &mut egui::Ui, idle: bool) -> Vec<egui::Rect> {
    let mut rects = Vec::new();
    // saved personas, plus any Docker still has from before (e.g. made with the command line)
    let ids: BTreeSet<String> = app
        .personas
        .saved
        .keys()
        .chain(app.personas.rows.keys())
        .cloned()
        .collect();
    let docker_ok = app.personas.docker_error.is_none();
    for id in ids {
        let row = app.personas.rows.get(&id).cloned();
        let saved = app.personas.saved.get(&id).cloned();
        let running = row.as_ref().is_some_and(PersonaRow::running);
        let name = saved
            .as_ref()
            .map(|s| s.profile.full_name())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.clone());
        let status = match (&row, running) {
            (_, true) => "Running",
            (Some(_), false) => "Stopped",
            (None, _) => "Not started",
        };
        let last = saved
            .as_ref()
            .and_then(|s| s.last_started)
            .map_or("never".to_string(), ago);
        let selected = app.personas.viewing.as_deref() == Some(id.as_str());

        // the whole card is clickable to select the persona; its buttons keep their own clicks
        let card = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            egui::Frame::group(ui.style())
                .fill(ui.visuals().faint_bg_color)
                .stroke(if selected {
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(0xe8, 0xa3, 0x5a))
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                })
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&name).strong().size(16.0));
                            card_status(app, ui, &id, status, &last);
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            persona_actions(
                                app,
                                ui,
                                &id,
                                row.clone(),
                                saved.is_some(),
                                idle,
                                docker_ok,
                            );
                        });
                    });
                });
        });
        rects.push(card.response.rect);
        if card
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.personas.viewing = Some(id.clone());
        }
        ui.add_space(6.0);
    }
    rects
}

/// The card's buttons, laid out right to left.
fn persona_actions(
    app: &mut RidgeApp,
    ui: &mut egui::Ui,
    id: &str,
    row: Option<PersonaRow>,
    saved: bool,
    idle: bool,
    docker_ok: bool,
) {
    let running = row.as_ref().is_some_and(PersonaRow::running);
    if ui
        .add_enabled(
            idle,
            egui::Button::new(icons::with(icons::DELETE, "Delete…")),
        )
        .on_hover_text("Deletes the persona, its identity sheet and its browser profile")
        .clicked()
    {
        app.personas.confirm_delete = Some((id.to_string(), String::new()));
    }
    if row.is_some()
        && ui
            .add_enabled(
                idle && docker_ok,
                egui::Button::new(icons::with(icons::CLEAN, "Remove containers")),
            )
            .on_hover_text("Removes the containers; the saved browser profile is kept")
            .clicked()
    {
        app.personas.confirm_remove = Some(id.to_string());
    }
    if saved
        && ui
            .add_enabled(idle, egui::Button::new(icons::with(icons::EDIT, "Edit")))
            .clicked()
    {
        open_builder(app, Some(id.to_string()));
    }
    if running {
        if ui
            .add_enabled(idle, egui::Button::new(icons::with(icons::STOP, "Stop")))
            .clicked()
        {
            if let Ok(n) = PersonaNames::new(id) {
                act(app, id, "Stopping…", "Couldn't stop", async move {
                    Manager::connect().await?.stop(&n).await?;
                    Ok(())
                });
            }
        }
        if let Some(url) = row.as_ref().and_then(|r| r.gui_url.clone()) {
            if ui
                .button(icons::with(icons::BROWSER, "Open browser"))
                .on_hover_text(&url)
                .clicked()
            {
                let protections = app.personas.protections;
                let url = app
                    .personas
                    .saved
                    .get(id)
                    .map_or(url.clone(), |s| screen_url(&url, s, &protections));
                let _ = open::that_detached(&url);
            }
        }
    } else if saved
        && ui
            .add_enabled(
                idle && docker_ok,
                egui::Button::new(egui::RichText::new(icons::with(icons::START, "Start")).strong()),
            )
            .on_disabled_hover_text("Needs Docker running")
            .clicked()
    {
        app.personas.checklist = Some((id.to_string(), Checklist::default()));
    }
}

/// Before launching: confirm the VPN is on, since the persona uses this PC's connection.
fn start_checklist(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some((id, list)) = &mut app.personas.checklist else {
        return;
    };
    let id = id.clone();
    // the protections are checked as soon as the pop-up opens
    if !list.checks_started {
        list.checks_started = true;
        settings_view::start_protection_check(app);
    }
    let check = settings_view::protection_check(app);
    let checked = matches!(check, ProtectionCheck::Passed(_));
    let tz = app
        .personas
        .saved
        .get(&id)
        .map(|s| s.tz.trim().to_string())
        .unwrap_or_default();
    let Some((_, list)) = &mut app.personas.checklist else {
        return;
    };
    let (mut go, mut cancel, mut check_again) = (false, false, false);
    egui::Modal::new(egui::Id::unique("start_checklist")).show(ui.ctx(), |ui| {
        ui.set_width(460.0);
        ui.heading(format!("Before you start {id}"));
        ui.label("The persona's browser goes online through this computer's connection.");
        ui.add_space(6.0);
        ui.checkbox(&mut list.vpn_on, "My VPN is connected");
        ui.checkbox(
            &mut list.timezone_matches,
            format!("The timezone ({tz}) matches my VPN's country"),
        );
        ui.add_space(4.0);
        let red = egui::Color32::from_rgb(218, 54, 51);
        match &check {
            ProtectionCheck::Running => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking the protections from a test persona…");
                });
            }
            ProtectionCheck::Passed(n) => {
                ui.colored_label(
                    egui::Color32::from_rgb(46, 160, 67),
                    icons::with(
                        icons::CHECK,
                        &format!("Protections checked: all {n} checks passed"),
                    ),
                );
            }
            ProtectionCheck::Failed(lines) => {
                ui.colored_label(
                    red,
                    icons::with(
                        icons::WARNING,
                        &format!("{} protection checks failed:", lines.len()),
                    ),
                );
                for line in lines {
                    ui.label(format!("    {line}"));
                }
                ui.weak("Turn the protections on in Settings (and Apply), then check again.");
            }
            ProtectionCheck::Error(e) => {
                ui.colored_label(
                    red,
                    icons::with(icons::ERROR, "The protections couldn't be checked"),
                )
                .on_hover_text(e);
            }
        }
        if matches!(
            check,
            ProtectionCheck::Failed(_) | ProtectionCheck::Error(_)
        ) && ui
            .button(icons::with(icons::RETRY, "Check again"))
            .clicked()
        {
            check_again = true;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    list.vpn_on && list.timezone_matches && checked,
                    egui::Button::new(
                        egui::RichText::new(icons::with(icons::START, "Start persona")).strong(),
                    ),
                )
                .on_disabled_hover_text("Tick both lines and wait for the protections to pass")
                .clicked()
            {
                go = true;
            }
            if ui.button(icons::with(icons::CANCEL, "Cancel")).clicked() {
                cancel = true;
            }
        });
    });
    if check_again {
        settings_view::start_protection_check(app);
    }
    if go || cancel {
        app.personas.checklist = None;
    }
    if go {
        start(app, &id);
    }
}

/// A fresh password for a persona's remote screen: 24 letters and digits from the OS's
/// randomness (`RandomState` is seeded by it), so it never needs escaping in a URL.
fn new_password() -> String {
    use std::hash::{BuildHasher, Hasher};
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    (0..24)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_usize(i);
            CHARS[(h.finish() % CHARS.len() as u64) as usize] as char
        })
        .collect()
}

/// The persona's screen address, with its password in it when the screen is protected.
fn screen_url(url: &str, s: &Saved, protections: &Protections) -> String {
    match url.strip_prefix("http://") {
        Some(rest) if protections.password && !s.password.is_empty() => {
            format!("http://{GUI_USER}:{}@{rest}", s.password)
        }
        _ => url.to_string(),
    }
}

/// Start arguments of saved persona `id`, with the protections of the Settings tab; gives the
/// persona a screen password the first time.
fn start_args(app: &mut RidgeApp, id: &str) -> Option<(StartArgs, Saved)> {
    let protections = app.personas.protections;
    let saved = app.personas.saved.get_mut(id)?;
    if saved.password.is_empty() {
        saved.password = new_password();
    }
    let s = saved.clone();
    let trimmed = |v: &str| (!v.trim().is_empty()).then(|| v.trim().to_string());
    let args = StartArgs {
        name: id.to_string(),
        vpn_country: None,
        vpn_provider: None,
        vpn_env_file: None,
        port: None,
        tz: trimmed(&s.tz).unwrap_or_else(|| "Etc/UTC".into()),
        bookmarks: s.bookmarks,
        image: DEFAULT_BROWSER_IMAGE.into(),
        vpn_image: DEFAULT_VPN_IMAGE.into(),
        pull: false,
        open: s.open_after,
        protections: Some(protections),
        gui_password: protections.password.then(|| s.password.clone()),
    };
    Some((args, s))
}

/// Starts persona `id` with its saved settings.
fn start(app: &mut RidgeApp, id: &str) {
    let Some((args, s)) = start_args(app, id) else {
        return;
    };
    if let Some(saved) = app.personas.saved.get_mut(id) {
        saved.last_started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
    }
    let protections = app.personas.protections;
    let opts = match crate::start_options(args) {
        Ok(opts) => opts,
        Err(err) => {
            app.personas.errors.insert(
                id.to_string(),
                ("Couldn't start".into(), format!("{err:#}")),
            );
            return;
        }
    };
    let doing = if app.personas.image_ready {
        "Starting…"
    } else {
        "Starting… (downloading the browser first)"
    };
    act(app, id, doing, "Couldn't start", async move {
        Manager::connect().await?.start(&opts).await?;
        let url = format!("http://127.0.0.1:{}", opts.host_port);
        // the container's browser needs a few seconds after start: open it once it answers
        wait_until_answering(&url, opts.gui_password.as_deref()).await;
        if s.open_after {
            let _ = open::that_detached(screen_url(&url, &s, &protections));
        }
        Ok(())
    });
}

/// The running personas Ridge saved (Settings applies and checks these).
pub(super) fn running_saved(app: &RidgeApp) -> Vec<String> {
    app.personas
        .saved
        .keys()
        .filter(|id| app.personas.rows.get(*id).is_some_and(PersonaRow::running))
        .cloned()
        .collect()
}

/// Restarts personas `ids` so the Settings protections apply, one after the other: each start
/// takes the first free local port, so the next one's options are made after it.
pub(super) fn apply_protections(app: &mut RidgeApp, ids: &[String]) {
    let all: Vec<StartArgs> = ids
        .iter()
        .filter_map(|id| start_args(app, id).map(|(args, _)| args))
        .collect();
    act(
        app,
        ALL_PERSONAS,
        "Restarting the personas…",
        "Couldn't apply",
        async move {
            let manager = Manager::connect().await?;
            for args in all {
                let name = args.name.clone();
                manager.clean(&PersonaNames::new(&name)?).await?;
                let opts = crate::start_options(args)?;
                manager
                    .start(&opts)
                    .await
                    .with_context(|| format!("restarting {name}"))?;
                let url = format!("http://127.0.0.1:{}", opts.host_port);
                wait_until_answering(&url, opts.gui_password.as_deref()).await;
            }
            Ok(())
        },
    );
}

/// Polls the persona's browser page until it answers, or the timeout passes.
async fn wait_until_answering(url: &str, password: Option<&str>) -> bool {
    let Ok(client) = health::client(Duration::from_secs(3)) else {
        return false;
    };
    let started = Instant::now();
    while started.elapsed() < BROWSER_READY_TIMEOUT {
        let mut request = client.get(url);
        if let Some(password) = password {
            request = request.basic_auth(GUI_USER, Some(password));
        }
        if let Ok(resp) = request.send().await {
            if resp.status().is_success() {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    false
}

fn delete_dialog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some((name, typed)) = &mut app.personas.confirm_delete else {
        return;
    };
    let mut close = false;
    let mut confirmed = false;
    egui::Modal::new(egui::Id::unique("delete_persona")).show(ui.ctx(), |ui| {
        ui.heading(format!("Delete {name}?"));
        ui.label(
            "This permanently deletes the persona: its identity sheet and its browser profile \
             (history, bookmarks and logins).",
        );
        ui.label(format!("Type {name} to confirm:"));
        ui.text_edit_singleline(typed);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    typed.trim() == name.as_str(),
                    egui::Button::new(icons::with(icons::DELETE, "Delete")),
                )
                .clicked()
            {
                confirmed = true;
            }
            if ui.button(icons::with(icons::CANCEL, "Cancel")).clicked() {
                close = true;
            }
        });
    });
    if confirmed {
        let name = name.clone();
        app.personas.confirm_delete = None;
        if let Some(s) = app.personas.saved.remove(&name) {
            forget_photo(&s.profile);
            notes::forget(&name);
        }
        if app.personas.viewing.as_deref() == Some(name.as_str()) {
            app.personas.viewing = None;
        }
        let in_docker = app.personas.rows.contains_key(&name);
        if in_docker {
            if let Ok(n) = PersonaNames::new(&name) {
                act(app, &name, "Deleting…", "Couldn't delete", async move {
                    Manager::connect().await?.purge(&n).await?;
                    Ok(())
                });
            }
        }
    } else if close {
        app.personas.confirm_delete = None;
    }
}

/// Confirms "Remove containers": the persona's browser profile volume is kept.
fn remove_dialog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some(id) = app.personas.confirm_remove.clone() else {
        return;
    };
    let (mut go, mut cancel) = (false, false);
    egui::Modal::new(egui::Id::unique("remove_containers")).show(ui.ctx(), |ui| {
        ui.set_width(420.0);
        ui.heading(format!("Remove the containers of {id}?"));
        ui.label(
            "Its browser is stopped and removed. Its browser profile and identity sheet are \
             kept, so you can start it again later.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(icons::with(icons::CLEAN, "Remove containers"))
                .clicked()
            {
                go = true;
            }
            if ui.button(icons::with(icons::CANCEL, "Cancel")).clicked() {
                cancel = true;
            }
        });
    });
    if go || cancel {
        app.personas.confirm_remove = None;
    }
    if go {
        if let Ok(n) = PersonaNames::new(&id) {
            act(
                app,
                &id,
                "Removing containers…",
                "Couldn't remove",
                async move {
                    Manager::connect().await?.clean(&n).await?;
                    Ok(())
                },
            );
        }
    }
}

/// The selected persona's identity sheet; values can be selected and copied.
fn profile_panel(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading("Profile");
    ui.add_space(4.0);
    let selected = app
        .personas
        .viewing
        .clone()
        .and_then(|id| app.personas.saved.get(&id).map(|s| (id, s.profile.clone())));
    let Some((id, profile)) = selected else {
        ui.weak("Click a persona to see its identity here.");
        return;
    };
    // the notes stay at the bottom, half the panel; the sheet above them scrolls
    let notes_h = (ui.available_height() * 0.5).max(280.0);
    egui::Panel::bottom("persona_notes")
        .resizable(false)
        .exact_size(notes_h)
        .show(ui, |ui| notes::show(app, ui, &id));
    let photo = profile.photo_file.clone();
    ui.horizontal(|ui| {
        photo_thumbnail(&mut app.personas.photos, ui, &photo, 72.0);
        ui.label(egui::RichText::new(profile.full_name()).strong().size(18.0));
    });
    egui::ScrollArea::vertical()
        .id_salt("profile_panel")
        .auto_shrink(false)
        .show(ui, |ui| {
            // each field with every value it has, one per line; records under their titles
            let mut sheet = profile.clone();
            let (parts, _) = sheet.parts();
            for part in parts {
                let (section, fields) = match part {
                    Part::Fields(section, fields) => (section, fields),
                    Part::Records(group, records) => {
                        records_panel(ui, group, records);
                        continue;
                    }
                };
                let filled: Vec<(&str, Vec<&str>)> = fields
                    .iter()
                    .map(|f| (f.label, profile.values(f.key, f.value)))
                    .filter(|(_, values)| !values.is_empty())
                    .collect();
                if filled.is_empty() {
                    continue;
                }
                ui.add_space(8.0);
                ui.strong(section);
                egui::Grid::new(("profile", section))
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for (label, values) in filled {
                            for (i, value) in values.into_iter().enumerate() {
                                if i == 0 {
                                    ui.weak(label);
                                } else {
                                    ui.label("");
                                }
                                ui.add(egui::Label::new(value).selectable(true).wrap());
                                ui.end_row();
                            }
                        }
                    });
            }
        });
}

/// A group's filled records in the Profile panel, each under its title.
fn records_panel(ui: &mut egui::Ui, group: &Group, records: &[Record]) {
    let filled: Vec<&Record> = records.iter().filter(|r| is_filled(r)).collect();
    if filled.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.strong(group.title);
    for (i, record) in filled.into_iter().enumerate() {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(group.record_title(record)).italics());
        egui::Grid::new(("profile", group.title, i))
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for field in group.fields {
                    let Some(value) = record.get(field.key).map(|v| v.trim()) else {
                        continue;
                    };
                    if value.is_empty() {
                        continue;
                    }
                    ui.weak(field.label);
                    ui.add(egui::Label::new(value).selectable(true).wrap());
                    ui.end_row();
                }
            });
    }
}

/// Where Ridge keeps its copies of profile pictures (removed by the uninstaller with the rest).
fn photo_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir("Ridge").map(|d| d.join("photos"))
}

/// Copies the chosen picture into Ridge's data folder, so the sheet keeps it if the original moves.
fn keep_photo(profile: &mut Profile, id: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    if profile.photo_file.is_empty() {
        return Ok(());
    }
    let src = std::path::PathBuf::from(&profile.photo_file);
    let dir = photo_dir().context("no data folder for profile pictures")?;
    if src.starts_with(&dir) {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).context("creating the profile picture folder")?;
    let ext = src
        .extension()
        .map_or("img".into(), |e| e.to_string_lossy().to_lowercase());
    let dest = dir.join(format!("{id}.{ext}"));
    std::fs::copy(&src, &dest)
        .with_context(|| format!("copying the profile picture {}", src.display()))?;
    profile.photo_file = dest.display().to_string();
    Ok(())
}

/// Deletes Ridge's copy of a deleted persona's picture (never the user's original).
fn forget_photo(profile: &Profile) {
    let path = std::path::PathBuf::from(&profile.photo_file);
    if photo_dir().is_some_and(|dir| path.starts_with(dir)) {
        let _ = std::fs::remove_file(path);
    }
}

/// A square thumbnail of `path`, or a placeholder when there is no readable picture.
fn photo_thumbnail(
    cache: &mut HashMap<String, Option<egui::TextureHandle>>,
    ui: &mut egui::Ui,
    path: &str,
    size: f32,
) {
    let texture = if path.is_empty() {
        None
    } else {
        cache
            .entry(path.to_string())
            .or_insert_with(|| {
                let img = image::open(path).ok()?.thumbnail(256, 256).to_rgba8();
                let pixels = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(pixels, img.as_raw());
                Some(
                    ui.ctx()
                        .load_texture(path, color, egui::TextureOptions::LINEAR),
                )
            })
            .clone()
    };
    match texture {
        Some(tex) => {
            ui.add(
                egui::Image::new((tex.id(), tex.size_vec2()))
                    .fit_to_exact_size(egui::vec2(size, size))
                    .maintain_aspect_ratio(true)
                    .corner_radius(8),
            );
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 8.0, ui.visuals().extreme_bg_color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if path.is_empty() {
                    icons::USER
                } else {
                    icons::ERROR
                },
                egui::FontId::proportional(size * 0.45),
                ui.visuals().weak_text_color(),
            );
        }
    }
}
