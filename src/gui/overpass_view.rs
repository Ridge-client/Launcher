//! "Overpass" tab: OpenStreetMap's Overpass API inside Ridge. A query on the left, what it finds
//! as markers and outlines over the same basemap (and layers) as the Tracking tab, a list of the
//! results with their tags, and GeoJSON export.

use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::map_view::{self, Viewport};
use super::{Msg, RidgeApp, NAV_ACCENT, NAV_MUTED};
use crate::overpass::{self, Feature};

/// Queries to start from; `{{bbox}}` is the part of the map in view.
const EXAMPLES: [(&str, &str); 9] = [
    (
        "Police stations",
        "[out:json][timeout:60];\nnwr[\"amenity\"=\"police\"]({{bbox}});\nout center tags;",
    ),
    (
        "Military areas",
        "[out:json][timeout:60];\n(\n  nwr[\"landuse\"=\"military\"]({{bbox}});\n  nwr[\"military\"]({{bbox}});\n);\nout geom;",
    ),
    (
        "Radio and cell masts",
        "[out:json][timeout:60];\n(\n  nwr[\"man_made\"=\"mast\"]({{bbox}});\n  nwr[\"man_made\"=\"tower\"][\"tower:type\"=\"communication\"]({{bbox}});\n);\nout center tags;",
    ),
    (
        "Surveillance cameras",
        "[out:json][timeout:60];\nnode[\"man_made\"=\"surveillance\"]({{bbox}});\nout tags;",
    ),
    (
        "Airports and helipads",
        "[out:json][timeout:60];\n(\n  nwr[\"aeroway\"=\"aerodrome\"]({{bbox}});\n  nwr[\"aeroway\"=\"helipad\"]({{bbox}});\n);\nout center tags;",
    ),
    (
        "Ports and ferry terminals",
        "[out:json][timeout:60];\n(\n  nwr[\"landuse\"=\"harbour\"]({{bbox}});\n  nwr[\"amenity\"=\"ferry_terminal\"]({{bbox}});\n);\nout center tags;",
    ),
    (
        "Embassies",
        "[out:json][timeout:60];\n(\n  nwr[\"amenity\"=\"embassy\"]({{bbox}});\n  nwr[\"office\"=\"diplomatic\"]({{bbox}});\n);\nout center tags;",
    ),
    (
        "Hospitals",
        "[out:json][timeout:60];\nnwr[\"amenity\"=\"hospital\"]({{bbox}});\nout center tags;",
    ),
    (
        "Search by name",
        "[out:json][timeout:60];\nnwr[\"name\"~\"hotel\",i]({{bbox}});\nout center tags;",
    ),
];

pub(super) struct State {
    query: String,
    /// Where the map looks; taken from the Tracking tab's the first time.
    view: Option<Viewport>,
    features: Vec<Feature>,
    selected: Option<usize>,
    running: bool,
    /// What the last run or export did; true when it went wrong.
    status: Option<(String, bool)>,
    canvas: Option<Rect>,
    /// The map is to be fitted to the results on the next frame.
    fit: bool,
    /// The map is to move to the chosen result on the next frame.
    fly: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: EXAMPLES[0].1.into(),
            view: None,
            features: Vec::new(),
            selected: None,
            running: false,
            status: None,
            canvas: None,
            fit: false,
            fly: false,
        }
    }
}

/// An Overpass run finished.
pub(super) fn loaded(app: &mut RidgeApp, result: Result<Vec<Feature>, String>) {
    let st = &mut app.overpass;
    st.running = false;
    st.selected = None;
    match result {
        Ok(features) => {
            st.status = Some((
                match features.len() {
                    0 => "Nothing found here".into(),
                    1 => "1 result".into(),
                    n => format!("{n} results"),
                },
                false,
            ));
            st.fit = !features.is_empty();
            st.features = features;
        }
        Err(e) => st.status = Some((e, true)),
    }
}

async fn run(client: reqwest::Client, query: String) -> Result<Vec<Feature>, String> {
    let response = client
        .post(overpass::ENDPOINT)
        .timeout(Duration::from_secs(90))
        .form(&[("data", query)])
        .send()
        .await
        .map_err(|e| format!("Couldn't reach Overpass: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("Couldn't read Overpass's answer: {e}"))?;
    if !status.is_success() {
        return Err(match status.as_u16() {
            400 => "Overpass didn't understand the query (check its syntax)".into(),
            429 | 504 => "Overpass is busy or the query took too long: try a smaller area".into(),
            _ => format!("Overpass answered {status}"),
        });
    }
    overpass::parse(&text).map_err(|e| format!("{e:#}"))
}

fn start(app: &mut RidgeApp, projection_bounds: ((f64, f64, f64, f64), (f64, f64))) {
    let (bbox, center) = projection_bounds;
    let query = overpass::prepare(&app.overpass.query, bbox, center);
    app.overpass.running = true;
    app.overpass.status = None;
    let client = app.map.client.clone();
    app.spawn(async move { Msg::Overpass(run(client, query).await) });
}

fn export(st: &mut State) {
    let Some(dest) = rfd::FileDialog::new()
        .set_title("Export the results")
        .add_filter("GeoJSON", &["geojson", "json"])
        .set_file_name("overpass.geojson")
        .save_file()
    else {
        return;
    };
    let text = serde_json::to_string_pretty(&overpass::geojson(&st.features)).unwrap_or_default();
    st.status = Some(match std::fs::write(&dest, text) {
        Ok(()) => (format!("Exported to {}", dest.display()), false),
        Err(e) => (format!("Couldn't export: {e}"), true),
    });
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    if app.overpass.view.is_none() {
        app.overpass.view = Some(Viewport {
            center: app.map.center,
            zoom: app.map.zoom,
        });
    }
    // what the map showed on the last frame is what {{bbox}} means
    let mut run_now = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let st = &mut app.overpass;
        let run = ui
            .add_enabled(
                !st.running,
                egui::Button::new(egui::RichText::new("▶  Run").strong().color(Color32::BLACK))
                    .fill(NAV_ACCENT),
            )
            .on_hover_text("Run the query over the map in view (Ctrl+Enter)");
        run_now |= run.clicked();
        ui.menu_button("Examples", |ui| {
            for (name, query) in EXAMPLES {
                if ui.button(name).clicked() {
                    st.query = query.into();
                    ui.close();
                }
            }
        });
        if ui
            .add_enabled(
                !st.features.is_empty(),
                egui::Button::new("Export GeoJSON…"),
            )
            .clicked()
        {
            export(st);
        }
        if ui
            .add_enabled(!st.features.is_empty(), egui::Button::new("Fit results"))
            .clicked()
        {
            st.fit = true;
        }
        if st.running {
            ui.spinner();
            ui.weak("asking Overpass…");
        }
        match &st.status {
            Some((text, true)) => {
                ui.colored_label(Color32::from_rgb(218, 54, 51), text);
            }
            Some((text, false)) => {
                ui.weak(text);
            }
            None => {}
        }
    });
    ui.add_space(6.0);

    egui::Panel::left("overpass_side")
        .resizable(true)
        .default_size(400.0)
        .size_range(280.0..=700.0)
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| side(&mut app.overpass, ui));

    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 8,
            right: 12,
            top: 0,
            bottom: 12,
        })
        .show(ui, |ui| {
            let bounds = canvas(app, ui);
            if run_now && !app.overpass.running {
                start(app, bounds);
            }
        });
}

/// The query editor, the results and the chosen result's tags.
fn side(st: &mut State, ui: &mut egui::Ui) {
    let total = ui.available_height();
    egui::ScrollArea::vertical()
        .id_salt("overpass_query")
        .max_height((total * 0.4).max(120.0))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut st.query)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(8)
                    .hint_text("An Overpass QL query"),
            );
        });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Results").strong());
        if !st.features.is_empty() {
            ui.weak(st.features.len().to_string());
        }
    });
    if st.features.is_empty() {
        ui.weak("Pick an example, move the map to the place and Run. {{bbox}} stands for the map in view.");
        return;
    }
    let row_h = ui.spacing().interact_size.y;
    let details_h = if st.selected.is_some() { 150.0 } else { 0.0 };
    egui::ScrollArea::vertical()
        .id_salt("overpass_results")
        .auto_shrink(false)
        .max_height((ui.available_height() - details_h - 12.0).max(60.0))
        .show_rows(ui, row_h, st.features.len(), |ui, rows| {
            for i in rows {
                let f = &st.features[i];
                let text = format!("{}  ·  {}", f.title(), f.kind);
                if ui
                    .add(
                        egui::Button::selectable(st.selected == Some(i), egui::RichText::new(text))
                            .truncate()
                            .min_size(Vec2::new(ui.available_width(), 0.0)),
                    )
                    .clicked()
                {
                    st.selected = Some(i);
                    st.fly = true;
                }
            }
        });
    if let Some(f) = st.selected.and_then(|i| st.features.get(i)) {
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(f.title()).strong());
                let url = format!("https://www.openstreetmap.org/{}/{}", f.kind, f.id);
                ui.hyperlink_to(format!("{}/{}", f.kind, f.id), url);
            });
            egui::ScrollArea::vertical()
                .id_salt("overpass_tags")
                .max_height(110.0)
                .show(ui, |ui| {
                    egui::Grid::new("overpass_tag_grid")
                        .num_columns(2)
                        .spacing([10.0, 2.0])
                        .show(ui, |ui| {
                            for (k, v) in &f.tags {
                                ui.label(egui::RichText::new(k).color(NAV_MUTED));
                                ui.add(egui::Label::new(v).wrap());
                                ui.end_row();
                            }
                        });
                });
        });
    }
}

/// Distance in points from `p` to the segment `a`–`b`.
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p - a).dot(ab)) / len2).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

/// The map with the results over it; returns what `{{bbox}}` and `{{center}}` mean right now.
fn canvas(app: &mut RidgeApp, ui: &mut egui::Ui) -> ((f64, f64, f64, f64), (f64, f64)) {
    let area = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    app.overpass.canvas = Some(area);
    let mut view = app.overpass.view.unwrap_or(Viewport {
        center: app.map.center,
        zoom: app.map.zoom,
    });
    if std::mem::take(&mut app.overpass.fit) {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for (lat, lon) in app.overpass.features.iter().flat_map(Feature::coords) {
            b = Some(match b {
                None => (lat, lat, lon, lon),
                Some((a, z, c, d)) => (a.min(lat), z.max(lat), c.min(lon), d.max(lon)),
            });
        }
        if let Some(b) = b {
            map_view::fit(&mut view, area.size(), b);
        }
    }
    if std::mem::take(&mut app.overpass.fly) {
        let chosen = app
            .overpass
            .selected
            .and_then(|i| app.overpass.features.get(i));
        if let Some(at) = chosen.and_then(Feature::anchor) {
            view.center = at;
            view.zoom = view.zoom.max(15.0);
        }
    }
    let projection = map_view::basemap(app, ui, area, &response, &mut view);
    app.overpass.view = Some(view);

    let st = &mut app.overpass;
    let pointer = response.hover_pos();
    let mut nearest: Option<(f32, usize)> = None;
    let outline = |selected: bool| {
        if selected {
            Stroke::new(4.0, Color32::WHITE)
        } else {
            Stroke::new(2.5, NAV_ACCENT)
        }
    };
    for (i, f) in st.features.iter().enumerate() {
        let selected = st.selected == Some(i);
        for line in &f.lines {
            let pts: Vec<Pos2> = line
                .iter()
                .map(|(lat, lon)| projection.to_screen(*lat, *lon))
                .collect();
            if pts.len() > 1 && !Rect::from_points(&pts).intersects(area) {
                continue;
            }
            if let Some(p) = pointer {
                for w in pts.windows(2) {
                    let d = segment_distance(p, w[0], w[1]);
                    if d < nearest.map_or(12.0, |n| n.0) {
                        nearest = Some((d, i));
                    }
                }
            }
            if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, outline(selected)));
            }
        }
        if let Some((lat, lon)) = f.point {
            let at = projection.to_screen(lat, lon);
            if !area.expand(10.0).contains(at) {
                continue;
            }
            if let Some(p) = pointer {
                let d = p.distance(at);
                if d < nearest.map_or(12.0, |n| n.0) {
                    nearest = Some((d, i));
                }
            }
            painter.circle_filled(at, if selected { 8.0 } else { 5.5 }, NAV_ACCENT);
            painter.circle_stroke(
                at,
                if selected { 8.0 } else { 5.5 },
                Stroke::new(
                    if selected { 2.5 } else { 1.5 },
                    if selected {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    },
                ),
            );
        }
    }
    if let Some((_, i)) = nearest {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        response.clone().on_hover_text(st.features[i].title());
    }
    if response.clicked() {
        st.selected = nearest.map(|(_, i)| i);
    }
    (projection.bounds(), view.center)
}
