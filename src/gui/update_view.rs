//! Updates in the window, as the Cairn launcher has them: nothing opens by itself. At start and
//! every few hours Ridge asks whether a newer release is out, and an orange dot by Versions in
//! the top bar says so. The Versions page's button opens the update window (or checks now):
//! what is new, Update now or Later; Update now shows the download's progress, then Ridge
//! closes and Setup opens the new one.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};

use super::{
    caps, graph_view, primary_button, secondary_button, Msg, RidgeApp, NAV_ACCENT, NAV_MUTED,
    NAV_TEXT,
};
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

/// The newer release, when one is out (an orange dot by Versions says so).
pub(super) fn available(app: &RidgeApp) -> Option<&Update> {
    app.update.info.as_ref()
}

pub(super) fn checking(app: &RidgeApp) -> bool {
    app.update.checking
}

/// The Versions page's button: the update window when one is out, else a check now.
pub(super) fn check_now(app: &mut RidgeApp) {
    if app.update.info.is_some() {
        app.update.dialog = Some(Stage::Offer);
    } else {
        check(app, true);
    }
}

/// Buttons in a row, centred: `widths` of each, so the row can be centred before it is laid
/// out.
fn centred_row<R>(ui: &mut egui::Ui, width: f32, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        body(ui)
    })
    .inner
}

/// A paragraph of the window: centred, wrapped.
fn line(ui: &mut egui::Ui, text: impl Into<String>, colour: Color32) {
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(text.into()).size(15.0).color(colour));
    });
}

/// The update window, as the Cairn launcher's: a title in spaced capitals, what is new or what
/// happened, then its buttons, all centred.
pub(super) fn dialog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let Some(stage) = &app.update.dialog else {
        return;
    };
    let info = app.update.info.clone();
    let unsaved = graph_view::has_unsaved(app);
    let (mut close, mut update_now, mut open_page) = (false, false, false);
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(0x14, 0x15, 0x17))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)))
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(32, 28));
    egui::Modal::new(egui::Id::unique("update_dialog"))
        .frame(frame)
        .backdrop_color(egui::Color32::from_black_alpha(170))
        .show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            let title = match (stage, &info) {
                (Stage::Current, _) => "Up to date".to_string(),
                (Stage::Unreachable, _) => "Can't check".into(),
                (Stage::Offer, Some(info)) => format!("{} is out", info.tag),
                (Stage::Busy { .. }, _) => "Updating".into(),
                (Stage::Failed(_), _) => "Update failed".into(),
                (Stage::Offer, None) => String::new(),
            };
            ui.vertical_centered(|ui| {
                ui.label(caps(&title, 19.0).strong().color(NAV_TEXT));
            });
            ui.add_space(14.0);
            match (stage, &info) {
                (Stage::Current, _) | (Stage::Unreachable, _) => {
                    if matches!(stage, Stage::Current) {
                        line(
                            ui,
                            format!(
                                "You have the newest version of Ridge, {}.",
                                env!("CARGO_PKG_VERSION")
                            ),
                            NAV_TEXT,
                        );
                    } else {
                        line(
                            ui,
                            "GitHub could not be reached. Check your connection and try again.",
                            NAV_TEXT,
                        );
                    }
                    ui.add_space(20.0);
                    close = centred_row(ui, 140.0, |ui| ui.add(primary_button("Close").min_size(egui::vec2(140.0, 40.0))).clicked());
                }
                (Stage::Offer, Some(info)) => {
                    line(ui, format!("You have {}.", env!("CARGO_PKG_VERSION")), NAV_MUTED);
                    if !info.notes.is_empty() {
                        ui.add_space(10.0);
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(&info.notes).size(14.5).color(NAV_TEXT));
                            });
                    }
                    ui.add_space(10.0);
                    line(
                        ui,
                        "Ridge closes, installs it over this copy and opens again. Your personas, graphs and settings stay.",
                        NAV_MUTED,
                    );
                    if unsaved {
                        ui.add_space(6.0);
                        line(
                            ui,
                            "The graph has unsaved changes: save it first (Ctrl+S on the Graph tab).",
                            ERROR,
                        );
                    }
                    ui.add_space(20.0);
                    (update_now, close) = centred_row(ui, 180.0 + 12.0 + 120.0, |ui| {
                        let now = ui
                            .add_enabled(!unsaved, primary_button("Update now").min_size(egui::vec2(180.0, 40.0)))
                            .clicked();
                        ui.add_space(12.0);
                        let later = ui.add(secondary_button("Later")).clicked();
                        (now, later)
                    });
                }
                (Stage::Busy { progress, text }, _) => {
                    line(ui, text.as_str(), NAV_TEXT);
                    ui.add_space(14.0);
                    match progress {
                        Some(p) => {
                            // a thin bar, amber on a dark track
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 8.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(rect, 4.0, egui::Color32::from_white_alpha(18));
                            let mut done = rect;
                            done.set_width(rect.width() * p.clamp(0.0, 1.0));
                            ui.painter().rect_filled(done, 4.0, NAV_ACCENT);
                        }
                        None => {
                            ui.vertical_centered(|ui| ui.spinner());
                        }
                    }
                    ui.add_space(14.0);
                    line(ui, "Please don't close Ridge.", NAV_MUTED);
                }
                (Stage::Failed(text), _) => {
                    line(ui, text.as_str(), NAV_TEXT);
                    ui.add_space(20.0);
                    let width = if info.is_some() { 260.0 + 12.0 + 120.0 } else { 120.0 };
                    (open_page, close) = centred_row(ui, width, |ui| {
                        let page = info.is_some()
                            && ui
                                .add(primary_button("Open the release page").min_size(egui::vec2(260.0, 40.0)))
                                .clicked();
                        if info.is_some() {
                            ui.add_space(12.0);
                        }
                        let close = ui.add(secondary_button("Close")).clicked();
                        (page, close)
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
