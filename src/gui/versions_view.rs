//! "Versions" page, as the Cairn launcher has it: the version running with a button to check
//! for (or install) an update, and the changelog of every release, read from GitHub when the
//! page first opens.

use eframe::egui::{self, Color32};

use super::{
    caps, card, icons, page_title, primary_button, update_view, Msg, RidgeApp, NAV_ACCENT,
    NAV_MUTED, NAV_TEXT,
};
use crate::update::{self, Release};

const YOU_ARE_HERE: Color32 = Color32::from_rgb(0x7e, 0xc8, 0x8a);

#[derive(Default)]
pub(super) struct State {
    /// The releases once read, or why they could not be.
    releases: Option<Result<Vec<Release>, String>>,
    loading: bool,
}

impl State {
    pub(super) fn loaded(&mut self, result: Result<Vec<Release>, String>) {
        self.loading = false;
        self.releases = Some(result);
    }
}

fn load(app: &mut RidgeApp) {
    if app.versions.loading {
        return;
    }
    app.versions.loading = true;
    app.spawn(async { Msg::Releases(update::releases().await) });
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    if app.versions.releases.is_none() {
        load(app);
    }
    egui::ScrollArea::vertical()
        .id_salt("versions")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(28, 0))
                .show(ui, |ui| {
                    page_title(
                        ui,
                        "Versions",
                        "What you are running, and what changed in each release.",
                    );
                    running(app, ui);
                    ui.add_space(14.0);
                    changelog(app, ui);
                    ui.add_space(24.0);
                });
        });
}

/// Ridge and its version; the button checks for an update, or installs the one that is out.
fn running(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let update = update_view::available(app).map(|u| u.tag.clone());
    let checking = update_view::checking(app);
    let clicked = card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(160.0, 40.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(160.0);
                    ui.label(egui::RichText::new("Ridge").size(16.0).color(NAV_MUTED))
                },
            );
            ui.label(
                egui::RichText::new(env!("CARGO_PKG_VERSION"))
                    .size(16.0)
                    .color(NAV_TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = match (&update, checking) {
                    (Some(tag), _) => format!("Update to {tag}"),
                    (None, true) => "Checking…".into(),
                    (None, false) => "Check for updates".into(),
                };
                ui.add_enabled(!checking, primary_button(&text)).clicked()
            })
            .inner
        })
        .inner
    });
    if clicked {
        update_view::check_now(app);
        // a release made since the page opened shows up too
        load(app);
    }
}

/// Every release, newest first: its version, the day it came out and what changed.
fn changelog(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let current = env!("CARGO_PKG_VERSION");
    let mut retry = false;
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(caps("Changelog", 15.0).strong().color(NAV_TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let link = ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Open on GitHub")
                                .size(15.0)
                                .color(NAV_ACCENT)
                                .underline(),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if link.clicked() {
                    let _ = open::that(update::RELEASES_PAGE);
                }
            });
        });
        ui.add_space(8.0);
        match &app.versions.releases {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Reading the releases…").color(NAV_MUTED));
                });
            }
            Some(Err(_)) => {
                ui.label(
                    egui::RichText::new(
                        "GitHub could not be reached, so the changelog can't show.",
                    )
                    .color(NAV_MUTED),
                );
                retry = ui.button(icons::with(icons::RETRY, "Try again")).clicked();
            }
            Some(Ok(list)) if list.is_empty() => {
                ui.label(egui::RichText::new("No release yet.").color(NAV_MUTED));
            }
            Some(Ok(list)) => {
                for (i, r) in list.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);
                    }
                    release(ui, r, update::version(&r.tag) == update::version(current));
                }
            }
        }
    });
    if retry {
        app.versions.releases = None;
    }
}

fn release(ui: &mut egui::Ui, r: &Release, here: bool) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&r.tag)
                .size(22.0)
                .strong()
                .color(NAV_TEXT),
        );
        if here {
            egui::Frame::new()
                .fill(YOU_ARE_HERE.gamma_multiply(0.18))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(9, 3))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("You are here")
                            .size(13.0)
                            .color(YOU_ARE_HERE),
                    );
                });
        }
        if let Some(day) = r.day {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(crate::sun::format_date_long(day))
                        .size(14.0)
                        .color(NAV_MUTED),
                );
            });
        }
    });
    ui.add_space(4.0);
    if r.notes.is_empty() {
        ui.label(egui::RichText::new("No notes.").color(NAV_MUTED));
    }
    for note in &r.notes {
        ui.horizontal_top(|ui| {
            ui.label(egui::RichText::new("•").size(15.0).color(NAV_TEXT));
            ui.add(egui::Label::new(egui::RichText::new(note).size(15.0).color(NAV_TEXT)).wrap());
        });
    }
}
