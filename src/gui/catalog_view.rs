//! "OSINT tools" tab: search, filter, check status and open tools.

use std::time::Duration;

use eframe::egui;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};

use super::{badge, icons, Msg, RidgeApp, Statuses};
use crate::catalog::health::{self, Status};
use crate::catalog::KINDS;

const CHECK_CONCURRENCY: usize = 32;
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const STORAGE_KEY: &str = "link_check";

#[derive(Default)]
pub(super) struct State {
    search: String,
    category: Option<String>,
    /// Only tools of this type (website, GitHub tool, …).
    kind: Option<&'static str>,
    pub(super) statuses: Statuses,
    /// (done, total) while a check runs.
    pub(super) progress: Option<(usize, usize)>,
    /// How many sites the last finished check covered.
    last_check: Option<usize>,
    hide_down: bool,
}

/// What survives a restart (eframe storage: %APPDATA%\Ridge on Windows).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    statuses: Statuses,
    last_check: Option<usize>,
    hide_down: bool,
}

impl State {
    pub(super) fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let saved: Saved = storage
            .and_then(|s| eframe::get_value(s, STORAGE_KEY))
            .unwrap_or_default();
        Self {
            statuses: saved.statuses,
            last_check: saved.last_check,
            hide_down: saved.hide_down,
            ..Self::default()
        }
    }

    pub(super) fn save(&self, storage: &mut dyn eframe::Storage) {
        let saved = Saved {
            statuses: self.statuses.clone(),
            last_check: self.last_check,
            hide_down: self.hide_down,
        };
        eframe::set_value(storage, STORAGE_KEY, &saved);
    }

    /// A check just finished: keep its total for the bar.
    pub(super) fn finish_check(&mut self) {
        if let Some((_, total)) = self.progress.take() {
            self.last_check = Some(total);
        }
    }
}

/// At start: check every site, while the last saved results stay on screen.
pub(super) fn check_on_start(app: &mut RidgeApp) {
    let targets = app
        .catalog
        .entries()
        .map(|e| (e.tool.name.clone(), e.tool.url.clone()))
        .collect();
    start_check(app, targets);
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.tools.search)
                .hint_text(icons::with(icons::SEARCH, "Search tools…"))
                .desired_width(260.0),
        );
        let selected = app
            .tools
            .category
            .clone()
            .unwrap_or_else(|| "All categories".into());
        egui::ComboBox::from_id_salt("category")
            .width(260.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.tools.category, None, "All categories");
                for c in &app.catalog.categories {
                    ui.selectable_value(&mut app.tools.category, Some(c.name.clone()), &c.name);
                }
            });
        egui::ComboBox::from_id_salt("kind")
            .width(170.0)
            .selected_text(app.tools.kind.unwrap_or("All types"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.tools.kind, None, "All types");
                for kind in KINDS {
                    ui.selectable_value(&mut app.tools.kind, Some(kind), kind);
                }
            });
        ui.checkbox(&mut app.tools.hide_down, "Hide down sites");
    });

    // every site is checked at start; the bar stays afterwards with the result
    ui.add_space(4.0);
    match (app.tools.progress, app.tools.last_check) {
        (Some((done, total)), _) => {
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .text(format!("Checking sites… {done} of {total}")),
            );
        }
        (None, Some(total)) => {
            ui.add(egui::ProgressBar::new(1.0).text(format!("{total} sites checked")));
        }
        (None, None) => {}
    }

    let mut entries = app
        .catalog
        .filter(app.tools.category.as_deref(), non_empty(&app.tools.search));
    if let Some(kind) = app.tools.kind {
        entries.retain(|e| e.tool.kind() == kind);
    }
    let listed = entries.len();
    let all_listed = entries.clone();
    let count = |f: fn(&Status) -> bool| {
        all_listed
            .iter()
            .filter(|e| app.tools.statuses.get(&e.tool.name).is_some_and(f))
            .count()
    };
    if app.tools.hide_down {
        let statuses = &app.tools.statuses;
        entries.retain(|e| !statuses.get(&e.tool.name).is_some_and(Status::is_down));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.weak(format!("{listed} tools"));
        if !app.tools.statuses.is_empty() {
            ui.label(badge(Some(&Status::Active { code: 0 })));
            ui.label(count(|s| matches!(s, Status::Active { .. })).to_string());
            ui.label(badge(Some(&Status::Guarded { code: 0 })));
            ui.label(count(|s| matches!(s, Status::Guarded { .. })).to_string());
            ui.label(badge(Some(&Status::Down {
                reason: String::new(),
            })));
            ui.label(count(Status::is_down).to_string());
        }
    });
    ui.separator();

    // Fixed-width columns; the description takes what is left. Only the rows on screen are drawn,
    // so the full catalog (1,700+ tools) scrolls smoothly.
    const STATUS_W: f32 = 90.0;
    const NAME_W: f32 = 230.0;
    const CATEGORY_W: f32 = 250.0;
    const TYPE_W: f32 = 130.0;
    const OPEN_W: f32 = 90.0;
    const ROW_H: f32 = 26.0;
    // a fixed-width, left-aligned, single-line cell
    let cell = |ui: &mut egui::Ui, w: f32, widget: egui::Label| {
        ui.allocate_ui_with_layout(
            egui::vec2(w, ROW_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(w);
                ui.add(widget.truncate())
            },
        )
        .inner
    };

    ui.horizontal(|ui| {
        cell(
            ui,
            STATUS_W,
            egui::Label::new(egui::RichText::new("Status").strong()),
        );
        cell(
            ui,
            NAME_W,
            egui::Label::new(egui::RichText::new("Tool").strong()),
        );
        cell(
            ui,
            CATEGORY_W,
            egui::Label::new(egui::RichText::new("Category").strong()),
        );
        cell(
            ui,
            TYPE_W,
            egui::Label::new(egui::RichText::new("Type").strong()),
        );
        ui.strong("Description");
    });

    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        ROW_H,
        entries.len(),
        |ui, rows| {
            for i in rows {
                let e = &entries[i];
                let status = app.tools.statuses.get(&e.tool.name);
                // reserved before the row so the stripe is painted underneath its text
                let stripe = ui.painter().add(egui::Shape::Noop);
                let row = ui.horizontal(|ui| {
                    ui.set_height(ROW_H);
                    let hover = match status {
                        Some(s) => crate::ui::detail(s),
                        None => "Not checked yet".into(),
                    };
                    cell(ui, STATUS_W, egui::Label::new(badge(status))).on_hover_text(hover);
                    cell(
                        ui,
                        NAME_W,
                        egui::Label::new(egui::RichText::new(&e.tool.name).strong()),
                    )
                    .on_hover_text(&e.tool.url);
                    cell(
                        ui,
                        CATEGORY_W,
                        egui::Label::new(egui::RichText::new(e.category).weak()),
                    )
                    .on_hover_text(e.category);
                    // a website to visit, or a tool to download and run (GitHub, PyPI, …)
                    cell(
                        ui,
                        TYPE_W,
                        egui::Label::new(egui::RichText::new(e.tool.kind()).weak()),
                    )
                    .on_hover_text(&e.tool.url);
                    let desc_w = (ui.available_width() - OPEN_W - 12.0).max(80.0);
                    cell(ui, desc_w, egui::Label::new(&e.tool.description))
                        .on_hover_text(&e.tool.description);
                    let down = status.is_some_and(Status::is_down);
                    // pinned to the right edge, whatever the description's width
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let open = ui
                            .add_enabled(!down, egui::Button::new(icons::with(icons::OPEN, "Open")))
                            .on_hover_text(&e.tool.url)
                            .on_disabled_hover_text("This site is down");
                        if open.clicked() {
                            let _ = open::that_detached(&e.tool.url);
                        }
                    });
                });
                if i % 2 == 1 {
                    let fill = ui.visuals().faint_bg_color;
                    ui.painter().set(
                        stripe,
                        egui::Shape::rect_filled(row.response.rect, 0.0, fill),
                    );
                }
            }
        },
    );
}

fn non_empty(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

/// Checks the given (name, url) pairs, streaming each result back to the window.
fn start_check(app: &mut RidgeApp, targets: Vec<(String, String)>) {
    if targets.is_empty() {
        return;
    }
    app.tools.progress = Some((0, targets.len()));
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.spawn(async move {
        if let Ok(client) = health::client(CHECK_TIMEOUT) {
            stream::iter(targets)
                .for_each_concurrent(CHECK_CONCURRENCY, |(name, url)| {
                    let (client, tx, ctx) = (client.clone(), tx.clone(), ctx.clone());
                    async move {
                        let status = health::check_url(&client, &url).await;
                        let _ = tx.send(Msg::Checked { name, status });
                        ctx.request_repaint();
                    }
                })
                .await;
        }
        Msg::CheckFinished
    });
}
