//! Updates in the window, as the Cairn launcher has them: nothing opens by itself. At start and
//! every few hours Ridge asks whether a newer release is out, and an orange dot by its version
//! in the top bar says so. Clicking the version opens the update window (or checks now): what
//! is new, Update now or Later; Update now shows the download's progress, then Ridge closes and
//! Setup opens the new one.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};

use super::icons;
use super::{graph_view, Msg, RidgeApp, NAV_ACCENT, NAV_MUTED, NAV_TEXT};
use crate::update::{self, Update};

/// How often Ridge looks again while it stays open.
const LOOK_EVERY: Duration = Duration::from_secs(4 * 3600);
const ERROR: Color32 = Color32::from_rgb(218, 54, 51);

pub(super) struct State {
    /// The newer release, once one was seen.
    info: Option<Update>,
    checking: bool,
    next_check: Instant,
    dialog: Option<Stage>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            info: None,
            checking: false,
            // the first look is at start
            next_check: Instant::now(),
            dialog: None,
        }
    }
}

/// What the update window shows.
enum Stage {
    /// What is new, Update now or Later.
    Offer,
    /// Downloading (with its progress), then starting Setup.
    Busy {
        progress: Option<f32>,
        text: String,
    },
    Failed(String),
    /// Checked on request: this is the newest.
    Current,
    /// Checked on request: GitHub could not be reached.
    Unreachable,
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.checking || matches!(self.dialog, Some(Stage::Busy { .. }))
    }
}

/// Asks GitHub now; `asked` when the user clicked, so the answer opens the window.
fn check(app: &mut RidgeApp, asked: bool) {
    let st = &mut app.update;
    st.next_check = Instant::now() + LOOK_EVERY;
    if st.checking {
        return;
    }
    st.checking = true;
    app.spawn(async move {
        Msg::UpdateChecked {
            result: update::check().await,
            asked,
        }
    });
}

/// Looks again when it is time, and wakes the window then.
pub(super) fn tick(app: &mut RidgeApp) {
    let now = Instant::now();
    if now >= app.update.next_check {
        check(app, false);
    }
    let wait = app.update.next_check.saturating_duration_since(now);
    app.ctx.request_repaint_after(wait);
}

pub(super) fn checked(app: &mut RidgeApp, result: Result<Option<Update>, String>, asked: bool) {
    let st = &mut app.update;
    st.checking = false;
    match result {
        Ok(info) => {
            let found = info.is_some();
            st.info = info;
            if asked {
                st.dialog = Some(if found { Stage::Offer } else { Stage::Current });
            }
        }
        // a failed look is silence, unless the user asked
        Err(_) if asked => st.dialog = Some(Stage::Unreachable),
        Err(_) => {}
    }
}

pub(super) fn progress(app: &mut RidgeApp, p: f32) {
    if let (Some(Stage::Busy { progress, text }), Some(info)) =
        (&mut app.update.dialog, &app.update.info)
    {
        *progress = Some(p);
        *text = format!("Downloading {}: {:.0}%", info.tag, p * 100.0);
    }
}

/// The download finished: start Setup and close, so it can replace Ridge's files.
pub(super) fn downloaded(app: &mut RidgeApp, result: Result<PathBuf, String>) {
    let st = &mut app.update;
    let tag = st.info.as_ref().map_or("", |i| i.tag.as_str()).to_string();
    match result.and_then(|setup| update::install(&setup)) {
        Ok(()) => {
            st.dialog = Some(Stage::Busy {
                progress: Some(1.0),
                text: format!("Installing {tag}: Ridge closes and opens again by itself."),
            });
            app.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        Err(why) => {
            st.dialog = Some(Stage::Failed(format!(
                "The update could not be installed: {why}. You can download it by hand from the release page."
            )))
        }
    }
}

fn start(app: &mut RidgeApp) {
    let Some(info) = app.update.info.clone() else {
        return;
    };
    app.update.dialog = Some(Stage::Busy {
        progress: None,
        text: "Starting…".into(),
    });
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.spawn(async move {
        let result = update::download(&info, |p| {
            let _ = tx.send(Msg::UpdateProgress(p));
            ctx.request_repaint();
        })
        .await;
        Msg::UpdateDownloaded(result)
    });
}

/// The version at the right of the top bar: an orange dot by it when a newer release is out;
/// clicking it opens the update window, or checks now.
pub(super) fn version_label(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &app.update;
    let text = format!("v{}", env!("CARGO_PKG_VERSION"));
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text, font, NAV_MUTED);
    let dot = 8.0;
    let size = egui::vec2(galley.size().x + dot + 14.0, galley.size().y + 8.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let colour = if response.hovered() {
        NAV_TEXT
    } else {
        NAV_MUTED
    };
    let text_pos = egui::pos2(
        rect.right() - galley.size().x - 4.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(text_pos, galley, colour);
    if st.info.is_some() {
        ui.painter().circle_filled(
            egui::pos2(text_pos.x - dot, rect.center().y),
            dot / 2.0,
            NAV_ACCENT,
        );
    }
    let hint = match (&st.info, st.checking) {
        (Some(info), _) => format!("Ridge {} is out: click to update", info.tag),
        (None, true) => "Checking for updates…".into(),
        (None, false) => "Check for updates".into(),
    };
    if response.on_hover_text(hint).clicked() {
        if app.update.info.is_some() {
            app.update.dialog = Some(Stage::Offer);
        } else {
            check(app, true);
        }
    }
}

/// The update window, while there is something to show.
pub(super) fn dialog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some(stage) = &app.update.dialog else {
        return;
    };
    let info = app.update.info.clone();
    let unsaved = graph_view::has_unsaved(app);
    let (mut close, mut update_now, mut open_page) = (false, false, false);
    egui::Modal::new(egui::Id::unique("update_dialog")).show(ui.ctx(), |ui| {
        ui.set_width(440.0);
        match (stage, &info) {
            (Stage::Current, _) => {
                ui.heading("Ridge is up to date");
                ui.label(format!(
                    "You have the newest version, {}.",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.add_space(6.0);
                close = ui.button("Close").clicked();
            }
            (Stage::Unreachable, _) => {
                ui.heading("Couldn't check for updates");
                ui.label("GitHub could not be reached. Check your connection and try again.");
                ui.add_space(6.0);
                close = ui.button("Close").clicked();
            }
            (Stage::Offer, Some(info)) => {
                ui.heading(format!("Ridge {} is out", info.tag));
                ui.weak(format!("You have {}.", env!("CARGO_PKG_VERSION")));
                if !info.notes.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| ui.label(&info.notes));
                }
                ui.add_space(6.0);
                ui.weak("Ridge closes, installs it over this copy and opens again. Your personas, graphs and settings stay.");
                if unsaved {
                    ui.colored_label(
                        ERROR,
                        "The graph has unsaved changes: save it first (Ctrl+S on the Graph tab).",
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    update_now = ui
                        .add_enabled(
                            !unsaved,
                            egui::Button::new(icons::with(icons::DOWNLOAD, "Update now")),
                        )
                        .clicked();
                    close = ui.button("Later").clicked();
                });
            }
            (Stage::Busy { progress, text }, _) => {
                ui.heading("Updating");
                ui.label(text);
                if let Some(p) = progress {
                    ui.add(egui::ProgressBar::new(*p));
                } else {
                    ui.add(egui::Spinner::new());
                }
                ui.weak("Please don't close Ridge.");
            }
            (Stage::Failed(text), _) => {
                ui.heading("The update failed");
                ui.colored_label(ERROR, text);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    open_page = info.is_some()
                        && ui
                            .button(icons::with(icons::OPEN, "Open the release page"))
                            .clicked();
                    close = ui.button("Close").clicked();
                });
            }
            (Stage::Offer, None) => close = true,
        }
    });
    if update_now {
        start(app);
    } else if open_page {
        if let Some(info) = &info {
            let _ = open::that(&info.page);
        }
    }
    if close {
        app.update.dialog = None;
    }
}
