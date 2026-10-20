//! Native desktop window (egui/eframe): the OSINT catalog and the persona manager with buttons.
//!
//! Slow work (health checks, Docker calls) runs on a tokio runtime in the background and reports
//! back through a channel, so the window never freezes.

mod annotate_view;
mod browser;
mod catalog_view;
mod graph_view;
mod icons;
mod persona_view;
mod settings_view;
mod sun_view;
mod zoom;

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use eframe::egui;

use crate::catalog::health::Status;
use crate::catalog::Catalog;

/// Results sent from background tasks to the window.
enum Msg {
    Checked {
        name: String,
        status: Status,
    },
    CheckFinished,
    Personas(Result<BTreeMap<String, PersonaRow>, String>),
    /// A persona action finished; `error` is (short note, full error) if it failed.
    Done {
        id: String,
        error: Option<(String, String)>,
    },
    /// Download of the persona browser image: progress (0..1), then the outcome.
    ImageProgress(f32),
    ImageReady(Result<(), String>),
    /// The Settings tab's checks of one persona finished.
    SettingsChecks {
        id: String,
        result: Result<Vec<crate::persona::firewall::Check>, String>,
    },
}

#[derive(Debug, Clone, Default)]
struct PersonaRow {
    browser: Option<String>,
    gui_url: Option<String>,
    /// What its browser was started with (none: an older Ridge started it, so nothing).
    protections: Option<crate::persona::firewall::Protections>,
}

impl PersonaRow {
    fn running(&self) -> bool {
        self.browser.as_deref().is_some_and(|s| s.starts_with("Up"))
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Tools,
    Personas,
    Graph,
    Annotate,
    Shadows,
    Settings,
}

pub fn run() -> Result<()> {
    let catalog_path = std::env::var_os("RIDGE_CATALOG").map(PathBuf::from);
    let catalog = Arc::new(Catalog::load(catalog_path.as_deref())?);
    let runtime = tokio::runtime::Runtime::new()?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Ridge")
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([820.0, 520.0]);
    // the Ridge icon for the window and taskbar (assets/ridge-icon.png)
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/ridge-icon.png"))
    {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Ridge",
        options,
        Box::new(move |cc| Ok(Box::new(RidgeApp::new(cc, runtime, catalog)))),
    )
    .map_err(|e| anyhow!("{e}"))
}

struct RidgeApp {
    ctx: egui::Context,
    runtime: tokio::runtime::Runtime,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    tab: Tab,
    catalog: Arc<Catalog>,
    tools: catalog_view::State,
    personas: persona_view::State,
    graph: graph_view::State,
    annotate: annotate_view::State,
    shadows: sun_view::State,
    settings: settings_view::State,
}

impl RidgeApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: tokio::runtime::Runtime,
        catalog: Arc<Catalog>,
    ) -> Self {
        annotate_view::forget_old_copies();
        let ctx = cc.egui_ctx.clone();
        ctx.set_theme(egui::Theme::Dark);
        icons::install(&ctx);
        let (tx, rx) = channel();
        let mut app = Self {
            ctx,
            runtime,
            tx,
            rx,
            tab: Tab::Tools,
            catalog,
            tools: catalog_view::State::load(cc.storage),
            personas: persona_view::State::load(cc.storage),
            graph: graph_view::State::load(cc.storage),
            annotate: annotate_view::State::default(),
            shadows: sun_view::State::load(cc.storage),
            settings: settings_view::State::default(),
        };
        catalog_view::check_on_start(&mut app);
        app
    }

    /// Runs `task` in the background; its message wakes the window when it arrives.
    fn spawn(&self, task: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.runtime.spawn(async move {
            let _ = tx.send(task.await);
            ctx.request_repaint();
        });
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Checked { name, status } => {
                    self.tools.statuses.insert(name, status);
                    if let Some((done, _)) = &mut self.tools.progress {
                        *done += 1;
                    }
                }
                Msg::CheckFinished => self.tools.finish_check(),
                Msg::Personas(result) => {
                    self.personas.loading = false;
                    match result {
                        Ok(list) => {
                            self.personas.rows = list;
                            self.personas.docker_error = None;
                            // get the browser ready before anyone presses Start
                            if persona_view::has_saved(self) {
                                persona_view::prepare_image(self);
                            }
                        }
                        Err(err) => self.personas.docker_error = Some(err),
                    }
                }
                Msg::ImageProgress(p) => self.personas.image_progress = Some(p),
                Msg::ImageReady(result) => {
                    self.personas.image_progress = None;
                    // on failure, starting a persona downloads the image itself
                    if result.is_ok() {
                        self.personas.image_ready = true;
                    }
                }
                Msg::Done { id, error } => {
                    self.personas.busy = None;
                    if let Some(error) = error {
                        self.personas.errors.insert(id, error);
                    }
                    persona_view::refresh(self);
                }
                Msg::SettingsChecks { id, result } => self.settings.checks_done(id, result),
            }
        }
    }
}

impl eframe::App for RidgeApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.tools.save(storage);
        self.personas.save(storage);
        self.graph.save(storage);
        self.shadows.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.handle_messages();

        egui::Panel::top("tabs")
            .exact_size(NAV_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let before = self.tab;
                    for (tab, label) in [
                        (Tab::Tools, "OSINT tools"),
                        (Tab::Personas, "Personas"),
                        (Tab::Graph, "Graph"),
                        (Tab::Annotate, "Annotate"),
                        (Tab::Shadows, "Shadows"),
                        (Tab::Settings, "Settings"),
                    ] {
                        if nav_button(ui, label, self.tab == tab).clicked() {
                            self.tab = tab;
                        }
                    }
                    // both pages show which personas run
                    if matches!(self.tab, Tab::Personas | Tab::Settings) && self.tab != before {
                        persona_view::refresh(self);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(12.0)
                                .color(NAV_MUTED),
                        );
                    });
                });
                // the hairline under the bar
                let r = ui.max_rect();
                ui.painter().hline(
                    r.x_range(),
                    r.bottom(),
                    egui::Stroke::new(1.0, egui::Color32::from_white_alpha(26)),
                );
            });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Tools => catalog_view::show(self, ui),
            Tab::Personas => persona_view::show(self, ui, frame),
            Tab::Graph => graph_view::show(self, ui),
            Tab::Annotate => annotate_view::show(self, ui),
            Tab::Shadows => sun_view::show(self, ui),
            Tab::Settings => settings_view::show(self, ui),
        });
        graph_view::guard_close(self, ui);
        // the built-in browser is a native window over ours: only the Personas tab shows it
        if self.tab != Tab::Personas {
            self.personas.browser.hide();
        }

        // keep spinners and progress bars moving while work runs in the background
        if self.tools.progress.is_some()
            || self.personas.busy.is_some()
            || self.personas.loading
            || self.personas.image_progress.is_some()
            || self.settings.busy()
        {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

const NAV_HEIGHT: f32 = 52.0;
const NAV_MUTED: egui::Color32 = egui::Color32::from_rgb(0x8a, 0x94, 0x9b);
const NAV_TEXT: egui::Color32 = egui::Color32::from_rgb(0xfb, 0xfb, 0xfa);
const NAV_ACCENT: egui::Color32 = egui::Color32::from_rgb(0xe8, 0xa3, 0x5a);

/// A navbar tab drawn like the Cairn launcher's: a text link the full height of the bar, white
/// with a short amber line on the bar's bottom edge when its page is open.
fn nav_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let font = egui::FontId::proportional(14.5);
    let text_color = |hovered: bool| {
        if active || hovered {
            NAV_TEXT
        } else {
            NAV_MUTED
        }
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), NAV_TEXT);
    // the full height of the bar, so the line under the open page sits on its bottom edge
    let size = egui::vec2(galley.size().x + 28.0, NAV_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if response.hovered() && !active {
        painter.rect_filled(rect, 0.0, egui::Color32::from_white_alpha(10));
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        text_color(response.hovered()),
    );
    if active {
        let line = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 12.0, rect.bottom() - 2.0),
            egui::pos2(rect.right() - 12.0, rect.bottom()),
        );
        painter.rect_filled(
            line,
            egui::CornerRadius {
                nw: 2,
                ne: 2,
                sw: 0,
                se: 0,
            },
            NAV_ACCENT,
        );
    }
    response
}

/// Colours of the status badges, shared by both views.
fn badge(status: Option<&Status>) -> egui::RichText {
    let (text, bg, fg) = match status {
        Some(Status::Active { .. }) => (
            " ACTIVE ",
            egui::Color32::from_rgb(46, 160, 67),
            egui::Color32::BLACK,
        ),
        Some(Status::Guarded { .. }) => (
            " GUARDED ",
            egui::Color32::from_rgb(210, 153, 34),
            egui::Color32::BLACK,
        ),
        Some(Status::Down { .. }) => (
            " DOWN ",
            egui::Color32::from_rgb(218, 54, 51),
            egui::Color32::WHITE,
        ),
        None => (
            " — ",
            egui::Color32::from_gray(60),
            egui::Color32::from_gray(200),
        ),
    };
    egui::RichText::new(text)
        .monospace()
        .strong()
        .background_color(bg)
        .color(fg)
}

type Statuses = HashMap<String, Status>;
