//! "Map" tab: imports NMEA, GPX, KML/KMZ and ADS-B SBS track files — a GPS log, an AIS feed, a
//! flight recorder, a car's trip history — and draws every one over a basemap, each file its
//! own colour. The basemap is OpenStreetMap's plain, key-free raster tiles, darkened locally
//! (see `darken`) the same way a browser's `invert(100%) hue-rotate(180deg)` CSS filter would,
//! so the map reads as part of this app's own dark theme without depending on a styled tile
//! service that can start gating its tiles behind an API key. Tiles are fetched over the
//! network and cached to disk (`%TEMP%/Ridge-tiles`), so panning the same area twice only asks
//! OpenStreetMap once.
//!
//! Unlike the Shadows tab's pictures (Ctrl+scroll to zoom, the plain scroll wheel free to pan),
//! this is a map: the scroll wheel and pinch zoom it, and the left button drags it, as every web
//! map does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::{icons, Msg, RidgeApp, NAV_MUTED, NAV_TEXT};
use crate::tracks::{self, Track};

const TILE_SIZE: f64 = 256.0;
const MIN_ZOOM: f32 = 2.0;
const MAX_ZOOM: f32 = 19.0;
const TILE_SERVER: &str = "https://tile.openstreetmap.org";
const TILE_PROVIDER: &str = "osm-dark";
const USER_AGENT: &str = concat!("Ridge/", env!("CARGO_PKG_VERSION"), " (desktop OSINT app)");
const EXTENSIONS: [&str; 6] = ["gpx", "kml", "kmz", "nmea", "log", "txt"];

/// A file's tracks all share one colour, cycling through this as files are added. Chosen to
/// read clearly over the dark basemap and against this app's dark panels alike.
const PALETTE: [Color32; 8] = [
    Color32::from_rgb(0xe8, 0xa3, 0x5a), // amber (the app's own accent)
    Color32::from_rgb(0x00, 0xac, 0xc1), // teal
    Color32::from_rgb(0xe5, 0x39, 0x35), // red
    Color32::from_rgb(0x7c, 0xb3, 0x42), // green
    Color32::from_rgb(0xb3, 0x9d, 0xdb), // violet
    Color32::from_rgb(0x42, 0x85, 0xf4), // blue
    Color32::from_rgb(0xe0, 0x61, 0xa8), // pink
    Color32::from_rgb(0xff, 0xca, 0x28), // yellow
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TileKey {
    z: u8,
    x: u32,
    y: u32,
}

enum TileState {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct LoadedFile {
    path: PathBuf,
    name: String,
    tracks: Vec<Track>,
    color: Color32,
    /// Shown on the map, and shown in the timeline below it: set independently, since a file
    /// can be worth seeing on one without cluttering the other.
    on_map: bool,
    in_timeline: bool,
}

impl LoadedFile {
    fn points(&self) -> usize {
        self.tracks.iter().map(|t| t.points.len()).sum()
    }

    /// (min lat, max lat, min lon, max lon), when it has at least one fix.
    fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        self.tracks
            .iter()
            .flat_map(|t| &t.points)
            .fold(None, |acc, f| {
                Some(match acc {
                    None => (f.lat, f.lat, f.lon, f.lon),
                    Some((a, b, c, d)) => (a.min(f.lat), b.max(f.lat), c.min(f.lon), d.max(f.lon)),
                })
            })
    }
}

/// The timeline's two series: each fix's altitude and speed, drawn in their own colour since
/// they share one chart but not one scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Metric {
    Altitude,
    Speed,
}

const ALTITUDE_COLOR: Color32 = Color32::from_rgb(0x42, 0x85, 0xf4);
const SPEED_COLOR: Color32 = Color32::from_rgb(0xe8, 0xa3, 0x5a);
/// How long a full playthrough of a track takes, whatever the track's own duration.
const PLAYBACK_SECONDS: f64 = 24.0;

impl Metric {
    fn color(self) -> Color32 {
        match self {
            Metric::Altitude => ALTITUDE_COLOR,
            Metric::Speed => SPEED_COLOR,
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Metric::Altitude => "m",
            Metric::Speed => "km/h",
        }
    }

    /// A fix's value in this metric's own unit (speed is kept in m/s on `Fix`; shown in km/h,
    /// the unit more people judge a speed by at a glance than m/s or knots).
    fn of(self, fix: &crate::tracks::Fix) -> Option<f64> {
        match self {
            Metric::Altitude => fix.alt,
            Metric::Speed => fix.speed.map(|s| s * 3.6),
        }
    }
}

pub(super) struct State {
    /// Where the map is centred, and how far in (fractional: a pinch or a scroll notch moves it
    /// smoothly, but tiles only exist at whole levels, so the nearest one is scaled to fit).
    center: (f64, f64),
    zoom: f32,
    tiles: HashMap<TileKey, TileState>,
    files: Vec<LoadedFile>,
    error: Option<String>,
    canvas: Option<Rect>,
    client: reqwest::Client,
    /// The timeline's moment, scrubbed to, dragged to or played to: the map then marks each
    /// track's fix closest to it instead of its last one.
    cursor_time: Option<i64>,
    playing: bool,
    /// How many seconds of the track pass per real second while playing.
    play_rate: f64,
}

impl Default for State {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            // Paris; as good a default as any until a file is opened
            center: (48.8566, 2.3522),
            zoom: 11.0,
            tiles: HashMap::new(),
            files: Vec::new(),
            error: None,
            canvas: None,
            client,
            cursor_time: None,
            playing: false,
            play_rate: 1.0,
        }
    }
}

// ---- Web Mercator: longitude/latitude <-> the slippy map's pixel space, at a zoom level that
// need not be a whole number (`lon_to_x`/`lat_to_y` scale smoothly with it either way) ----

fn lon_to_x(lon: f64, zoom: f64) -> f64 {
    let n = 2f64.powf(zoom);
    (lon + 180.0) / 360.0 * n * TILE_SIZE
}

fn lat_to_y(lat: f64, zoom: f64) -> f64 {
    let n = 2f64.powf(zoom);
    let lat = lat.clamp(-85.051_128, 85.051_128).to_radians();
    (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n * TILE_SIZE
}

fn x_to_lon(x: f64, zoom: f64) -> f64 {
    let n = 2f64.powf(zoom);
    x / (n * TILE_SIZE) * 360.0 - 180.0
}

fn y_to_lat(y: f64, zoom: f64) -> f64 {
    let n = 2f64.powf(zoom);
    let yf = y / (n * TILE_SIZE);
    (std::f64::consts::PI * (1.0 - 2.0 * yf))
        .sinh()
        .atan()
        .to_degrees()
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let dropped: Vec<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    for path in dropped {
        add_file(app, path);
    }

    egui::Panel::left("map_files")
        .resizable(false)
        .exact_size(300.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("map_files")
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(16, 0))
                        .show(ui, |ui| sidebar(app, ui));
                });
        });

    egui::Panel::bottom("map_timeline")
        .resizable(false)
        .exact_size(190.0)
        .show(ui, |ui| timeline(app, ui));

    canvas(app, ui);
}

fn sidebar(app: &mut RidgeApp, ui: &mut egui::Ui) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("Track files")
            .strong()
            .size(16.0)
            .color(NAV_TEXT),
    );
    ui.add_space(8.0);
    if ui
        .button(icons::with(icons::FOLDER_OPEN, "Add files…"))
        .on_hover_text("NMEA, GPX, KML/KMZ or an ADS-B SBS log (or drop one on the map)")
        .clicked()
    {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose track files")
            .add_filter("Track files", &EXTENSIONS)
            .add_filter("All files", &["*"])
            .pick_files()
        {
            for path in paths {
                add_file(app, path);
            }
        }
    }
    if let Some(err) = app.map.error.clone() {
        ui.add_space(6.0);
        ui.colored_label(Color32::from_rgb(218, 54, 51), err);
    }
    if app.map.files.is_empty() {
        ui.add_space(8.0);
        ui.weak("Nothing opened yet.");
        return;
    }

    let mut remove = None;
    let mut fit = None;
    ui.add_space(10.0);
    ui.separator();
    for (i, file) in app.map.files.iter_mut().enumerate() {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, file.color);
            ui.label(egui::RichText::new(&file.name).color(NAV_TEXT))
                .on_hover_text(file.path.display().to_string());
        });
        ui.label(
            egui::RichText::new(format!(
                "{} track{}, {} points",
                file.tracks.len(),
                if file.tracks.len() == 1 { "" } else { "s" },
                file.points()
            ))
            .size(12.0)
            .color(NAV_MUTED),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let small = |label: &str| egui::RichText::new(label).size(12.0).color(NAV_MUTED);
            ui.add(egui::Checkbox::new(&mut file.on_map, small("Map")))
                .on_hover_text("Show this file's tracks on the map");
            ui.add(egui::Checkbox::new(
                &mut file.in_timeline,
                small("Timeline"),
            ))
            .on_hover_text("Show this file's tracks in the timeline below");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.small_button(icons::with(icons::FIT, "Fit")).clicked() {
                fit = Some(i);
            }
            if ui
                .small_button(icons::CANCEL)
                .on_hover_text("Remove")
                .clicked()
            {
                remove = Some(i);
            }
        });
        ui.add_space(6.0);
        ui.separator();
    }
    if let Some(i) = fit {
        if let Some(bounds) = app.map.files[i].bounds() {
            fit_bounds(app, bounds);
        }
    }
    if let Some(i) = remove {
        app.map.files.remove(i);
    }
}

/// Below the map: Altitude/Speed over time for every visible track that has it, and the
/// moment scrubbed to, which marks each track's nearest fix on the map above instead of its
/// last one.
fn timeline(app: &mut RidgeApp, ui: &mut egui::Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            let bounds = chart(app, ui);
            ui.add_space(6.0);
            player(app, ui, bounds);
        });
}

/// A track's fixes, smoothed (a short moving average) so GPS jitter between close-together
/// points doesn't turn the line into noise too dense to read.
fn smoothed(points: &[(i64, f64)]) -> Vec<(i64, f64)> {
    const WINDOW: usize = 9;
    if points.len() <= WINDOW {
        return points.to_vec();
    }
    let half = WINDOW / 2;
    (0..points.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(points.len());
            let avg = points[lo..hi].iter().map(|p| p.1).sum::<f64>() / (hi - lo) as f64;
            (points[i].0, avg)
        })
        .collect()
}

/// The timeline's chart: altitude and speed over time, on their own vertical scale each but one
/// shared time axis; returns that time axis (`None` with nothing to plot).
fn chart(app: &mut RidgeApp, ui: &mut egui::Ui) -> Option<(i64, i64)> {
    let size = Vec2::new(ui.available_width(), ui.available_height() - 34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_black_alpha(70));

    let mut per_metric = [Vec::new(), Vec::new()]; // [altitude, speed] tracks' points
    let (mut tmin, mut tmax) = (i64::MAX, i64::MIN);
    let mut ranges = [(f64::MAX, f64::MIN), (f64::MAX, f64::MIN)];
    for file in app.map.files.iter().filter(|f| f.in_timeline) {
        for track in &file.tracks {
            for (m, metric) in [Metric::Altitude, Metric::Speed].into_iter().enumerate() {
                let points: Vec<(i64, f64)> = track
                    .points
                    .iter()
                    .filter_map(|f| Some((f.time?, metric.of(f)?)))
                    .collect();
                if points.len() < 2 {
                    continue;
                }
                for &(t, v) in &points {
                    tmin = tmin.min(t);
                    tmax = tmax.max(t);
                    ranges[m].0 = ranges[m].0.min(v);
                    ranges[m].1 = ranges[m].1.max(v);
                }
                per_metric[m].push(smoothed(&points));
            }
        }
    }
    if tmin >= tmax {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No altitude or speed data in these files",
            egui::FontId::proportional(13.0),
            NAV_MUTED,
        );
        return None;
    }
    for range in &mut ranges {
        if range.1 <= range.0 {
            range.1 = range.0 + 1.0;
        }
    }

    // room for the value labels above/below the line, so a peak or a trough is never clipped
    let plot = rect.shrink2(Vec2::new(4.0, 16.0));
    let x_of = |t: i64| plot.left() + (t - tmin) as f32 / (tmax - tmin) as f32 * plot.width();
    let y_of = |v: f64, (lo, hi): (f64, f64)| {
        plot.bottom() - ((v - lo) / (hi - lo)) as f32 * plot.height()
    };

    let small = egui::FontId::proportional(10.0);
    for (m, metric) in [Metric::Altitude, Metric::Speed].into_iter().enumerate() {
        if per_metric[m].is_empty() {
            continue;
        }
        for points in &per_metric[m] {
            let line: Vec<Pos2> = points
                .iter()
                .map(|&(t, v)| Pos2::new(x_of(t), y_of(v, ranges[m])))
                .collect();
            painter.add(egui::Shape::line(line, Stroke::new(1.6, metric.color())));
        }
        // altitude's labels on the left, speed's on the right, each by its own line's colour
        let (align, x) = if m == 0 {
            (egui::Align2::LEFT_BOTTOM, plot.left())
        } else {
            (egui::Align2::RIGHT_BOTTOM, plot.right())
        };
        painter.text(
            Pos2::new(x, plot.top() - 2.0),
            align,
            format!("{:.0} {}", ranges[m].1, metric.unit()),
            small.clone(),
            metric.color(),
        );
        let align = if m == 0 {
            egui::Align2::LEFT_TOP
        } else {
            egui::Align2::RIGHT_TOP
        };
        painter.text(
            Pos2::new(x, plot.bottom() + 2.0),
            align,
            format!("{:.0} {}", ranges[m].0, metric.unit()),
            small.clone(),
            metric.color(),
        );
    }

    // scrubbing: dragging or clicking moves the cursor and pauses any playback
    if let Some(p) = response.interact_pointer_pos() {
        let frac = ((p.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
        app.map.cursor_time = Some(tmin + (frac * (tmax - tmin) as f32) as i64);
        app.map.playing = false;
    }
    if let Some(t) = app.map.cursor_time.filter(|t| (tmin..=tmax).contains(t)) {
        let x = x_of(t);
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(1.5, Color32::from_white_alpha(180)),
        );
    }
    // hovering alone (no button down) still shows what moment is under the pointer
    if let Some(p) = response.hover_pos() {
        let frac = ((p.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
        let t = tmin + (frac * (tmax - tmin) as f32) as i64;
        painter.line_segment(
            [Pos2::new(p.x, plot.top()), Pos2::new(p.x, plot.bottom())],
            Stroke::new(1.0, Color32::from_white_alpha(70)),
        );
        painter.text(
            Pos2::new(
                p.x.clamp(plot.left() + 36.0, plot.right() - 36.0),
                plot.top() - 2.0,
            ),
            egui::Align2::CENTER_BOTTOM,
            format!("{} UTC", crate::sun::format_clock(t)),
            small,
            NAV_TEXT,
        );
    }

    Some((tmin, tmax))
}

/// How fast playback can run: a button cycles through these, in order.
const PLAY_RATES: [f64; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

fn next_play_rate(rate: f64) -> f64 {
    let i = PLAY_RATES
        .iter()
        .position(|r| (r - rate).abs() < 1e-9)
        .unwrap_or(2);
    PLAY_RATES[(i + 1) % PLAY_RATES.len()]
}

/// Jump to the start or end, play/pause, and the speed it plays at: always shown, even with
/// nothing loaded yet, so the controls don't jump around the panel once a file is.
fn player(app: &mut RidgeApp, ui: &mut egui::Ui, bounds: Option<(i64, i64)>) {
    if let (true, Some((tmin, tmax))) = (app.map.playing, bounds) {
        let rate = (tmax - tmin) as f64 / PLAYBACK_SECONDS * app.map.play_rate;
        let dt = ui.input(|i| i.stable_dt) as f64;
        let next = app.map.cursor_time.unwrap_or(tmin) + (dt * rate) as i64;
        if next >= tmax {
            app.map.cursor_time = Some(tmax);
            app.map.playing = false;
        } else {
            app.map.cursor_time = Some(next);
        }
        ui.ctx().request_repaint();
    }

    let enabled = bounds.is_some();
    let (tmin, tmax) = bounds.unwrap_or_default();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled, egui::Button::new(icons::PREVIOUS).small())
            .on_hover_text("Jump to the start")
            .clicked()
        {
            app.map.cursor_time = Some(tmin);
            app.map.playing = false;
        }
        let icon = if app.map.playing {
            icons::STOP
        } else {
            icons::START
        };
        if ui
            .add_enabled(enabled, egui::Button::new(icon).small())
            .on_hover_text(if app.map.playing { "Pause" } else { "Play" })
            .clicked()
        {
            if !app.map.playing && app.map.cursor_time.is_none_or(|t| t >= tmax) {
                app.map.cursor_time = Some(tmin);
            }
            app.map.playing = !app.map.playing;
        }
        if ui
            .add_enabled(enabled, egui::Button::new(icons::NEXT).small())
            .on_hover_text("Jump to the end")
            .clicked()
        {
            app.map.cursor_time = Some(tmax);
            app.map.playing = false;
        }
        ui.add_space(8.0);
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("{}×", fmt_rate(app.map.play_rate))).small(),
            )
            .on_hover_text("Playback speed")
            .clicked()
        {
            app.map.play_rate = next_play_rate(app.map.play_rate);
        }
        ui.add_space(8.0);
        if enabled {
            let t = app.map.cursor_time.unwrap_or(tmin).clamp(tmin, tmax);
            ui.weak(format!("{} UTC", crate::sun::format_clock(t)));
        } else {
            ui.weak("No timeline data yet");
        }
    });
}

fn fmt_rate(rate: f64) -> String {
    if rate.fract() == 0.0 {
        format!("{rate:.0}")
    } else {
        format!("{rate}")
    }
}

/// The index of `track`'s fix with a time closest to `t` (the track is assumed to have at
/// least one timed fix; its last point stands in if somehow none do).
fn nearest_by_time(track: &Track, t: i64) -> usize {
    track
        .points
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.time.map(|ft| (i, (ft - t).abs())))
        .min_by_key(|&(_, diff)| diff)
        .map_or(track.points.len().saturating_sub(1), |(i, _)| i)
}

fn add_file(app: &mut RidgeApp, path: PathBuf) {
    match tracks::load(&path) {
        Ok(found) => {
            let color = PALETTE[app.map.files.len() % PALETTE.len()];
            let name = file_name(&path);
            let fit_after = app.map.files.is_empty();
            app.map.files.push(LoadedFile {
                path,
                name,
                tracks: found,
                color,
                on_map: true,
                in_timeline: true,
            });
            app.map.error = None;
            if fit_after {
                if let Some(bounds) = app.map.files[0].bounds() {
                    fit_bounds(app, bounds);
                }
            }
        }
        Err(e) => app.map.error = Some(format!("{}: {e}", file_name(&path))),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Centres on, and zooms as far as possible into, a bounding box — a single point zooms in
/// close rather than all the way, since there is nothing more to see there.
fn fit_bounds(app: &mut RidgeApp, (min_lat, max_lat, min_lon, max_lon): (f64, f64, f64, f64)) {
    let size = app
        .map
        .canvas
        .map(|r| r.size())
        .unwrap_or(Vec2::new(900.0, 700.0));
    app.map.center = ((min_lat + max_lat) / 2.0, (min_lon + max_lon) / 2.0);
    if min_lat == max_lat && min_lon == max_lon {
        app.map.zoom = 14.0;
        return;
    }
    let mut zoom = MAX_ZOOM;
    while zoom > MIN_ZOOM {
        let w = (lon_to_x(max_lon, zoom as f64) - lon_to_x(min_lon, zoom as f64)).abs();
        let h = (lat_to_y(max_lat, zoom as f64) - lat_to_y(min_lat, zoom as f64)).abs();
        if w <= size.x as f64 * 0.9 && h <= size.y as f64 * 0.9 {
            break;
        }
        zoom -= 1.0;
    }
    app.map.zoom = zoom;
}

/// Loads (from the disk cache, or the tile server) the one tile `key` names, if it isn't
/// already loading, loaded or known to have failed.
fn ensure_tile(app: &mut RidgeApp, key: TileKey) {
    if app.map.tiles.contains_key(&key) {
        return;
    }
    app.map.tiles.insert(key, TileState::Loading);
    let client = app.map.client.clone();
    app.spawn(async move {
        let image = fetch_tile(client, key).await;
        Msg::TileLoaded { key, image }
    });
}

async fn fetch_tile(client: reqwest::Client, key: TileKey) -> Option<egui::ColorImage> {
    let cached = cache_path(key);
    let bytes = match std::fs::read(&cached) {
        Ok(bytes) => bytes,
        Err(_) => {
            let url = format!("{TILE_SERVER}/{}/{}/{}.png", key.z, key.x, key.y);
            let response = client.get(&url).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let bytes = response.bytes().await.ok()?.to_vec();
            if let Some(dir) = cached.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&cached, &bytes);
            bytes
        }
    };
    let decoded = darken(image::load_from_memory(&bytes).ok()?.to_rgba8());
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        decoded.as_raw(),
    ))
}

/// A plain OpenStreetMap tile, filtered into this app's dark theme the way a browser's CSS
/// `invert(100%) hue-rotate(180deg)` would: inverting turns the pale background near-black and
/// dark roads near-white, and the hue rotation undoes the colour drift invert alone would leave
/// (blue water turning orange, and so on), so roads, water and parks keep roughly their usual
/// hues, just darkened.
fn darken(mut image: image::RgbaImage) -> image::RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let (ri, gi, bi) = (255.0 - r as f32, 255.0 - g as f32, 255.0 - b as f32);
        let channel = |m: [f32; 3]| {
            (m[0] * ri + m[1] * gi + m[2] * bi)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        pixel.0 = [
            channel([-0.574, 1.430, 0.144]),
            channel([0.426, 0.430, 0.144]),
            channel([0.426, 1.430, -0.856]),
            a,
        ];
    }
    image
}

fn cache_path(key: TileKey) -> PathBuf {
    std::env::temp_dir()
        .join("Ridge-tiles")
        .join(TILE_PROVIDER)
        .join(key.z.to_string())
        .join(key.x.to_string())
        .join(format!("{}.png", key.y))
}

pub(super) fn tile_loaded(app: &mut RidgeApp, key: TileKey, image: Option<egui::ColorImage>) {
    let state = match image {
        Some(image) => {
            let name = format!("tile-{}-{}-{}", key.z, key.x, key.y);
            TileState::Ready(
                app.ctx
                    .load_texture(name, image, egui::TextureOptions::LINEAR),
            )
        }
        None => TileState::Failed,
    };
    app.map.tiles.insert(key, state);
}

fn canvas(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let area = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, Color32::from_rgb(0x1a, 0x1c, 0x1f));
    app.map.canvas = Some(area);

    let zoom = app.map.zoom as f64;
    let tile_zoom = (app.map.zoom.round().clamp(MIN_ZOOM, MAX_ZOOM)) as i32;
    // tile-space pixels, scaled up to this frame's (possibly in-between) zoom level
    let scale = 2f64.powf(zoom - tile_zoom as f64);
    let center_world = (
        lon_to_x(app.map.center.1, zoom),
        lat_to_y(app.map.center.0, zoom),
    );
    let to_screen = |wx: f64, wy: f64| -> Pos2 {
        Pos2::new(
            area.center().x + (wx - center_world.0) as f32,
            area.center().y + (wy - center_world.1) as f32,
        )
    };

    let tiles_across = 1i64 << tile_zoom;
    let tile_screen = (TILE_SIZE * scale) as f32;
    let half = Vec2::new(area.width() / 2.0, area.height() / 2.0) / scale as f32;
    let center_tiles = (
        center_world.0 / scale / TILE_SIZE,
        center_world.1 / scale / TILE_SIZE,
    );
    let x0 = (center_tiles.0 - half.x as f64 / TILE_SIZE).floor() as i64;
    let x1 = (center_tiles.0 + half.x as f64 / TILE_SIZE).floor() as i64;
    let y0 = ((center_tiles.1 - half.y as f64 / TILE_SIZE).floor() as i64).max(0);
    let y1 = ((center_tiles.1 + half.y as f64 / TILE_SIZE).floor() as i64).min(tiles_across - 1);

    for ty in y0..=y1 {
        for tx_raw in x0..=x1 {
            let tx = tx_raw.rem_euclid(tiles_across);
            let key = TileKey {
                z: tile_zoom as u8,
                x: tx as u32,
                y: ty as u32,
            };
            ensure_tile(app, key);
            let min = to_screen(
                tx_raw as f64 * TILE_SIZE * scale,
                ty as f64 * TILE_SIZE * scale,
            );
            let rect = Rect::from_min_size(min, Vec2::splat(tile_screen));
            match app.map.tiles.get(&key) {
                Some(TileState::Ready(tex)) => {
                    painter.image(
                        tex.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                _ => {
                    painter.rect_filled(
                        rect.intersect(area),
                        0.0,
                        Color32::from_rgb(0x26, 0x29, 0x2d),
                    );
                }
            }
        }
    }

    let cursor_time = app.map.cursor_time;
    for file in app.map.files.iter().filter(|f| f.on_map) {
        for track in &file.tracks {
            let points: Vec<Pos2> = track
                .points
                .iter()
                .map(|f| to_screen(lon_to_x(f.lon, zoom), lat_to_y(f.lat, zoom)))
                .collect();
            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(2.5, file.color),
                ));
            }
            // the timeline's scrubbed moment, when this track has times to match it against;
            // otherwise just its last fix, as usual
            let marked = cursor_time
                .filter(|_| track.points.iter().any(|f| f.time.is_some()))
                .map(|t| nearest_by_time(track, t))
                .unwrap_or(points.len().wrapping_sub(1));
            if let Some(&at) = points.get(marked) {
                painter.circle_filled(at, 5.0, file.color);
                painter.circle_stroke(at, 5.0, Stroke::new(1.5, Color32::BLACK));
            }
        }
    }

    if response.dragged() {
        let d = response.drag_delta();
        let moved = (center_world.0 - d.x as f64, center_world.1 - d.y as f64);
        app.map.center = (y_to_lat(moved.1, zoom), x_to_lon(moved.0, zoom));
    }

    if let Some(p) = response.hover_pos() {
        let (pinch, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y));
        let delta = if pinch != 1.0 {
            pinch.log2()
        } else if scroll.abs() > 0.5 {
            scroll.signum() * 0.4
        } else {
            0.0
        };
        if delta != 0.0 {
            let new_zoom = (app.map.zoom + delta).clamp(MIN_ZOOM, MAX_ZOOM);
            if new_zoom != app.map.zoom {
                let offset = (p.x - area.center().x, p.y - area.center().y);
                let anchor = (
                    x_to_lon(center_world.0 + offset.0 as f64, zoom),
                    y_to_lat(center_world.1 + offset.1 as f64, zoom),
                );
                app.map.zoom = new_zoom;
                let zoom2 = new_zoom as f64;
                let anchor_world = (lon_to_x(anchor.0, zoom2), lat_to_y(anchor.1, zoom2));
                let center_world2 = (
                    anchor_world.0 - offset.0 as f64,
                    anchor_world.1 - offset.1 as f64,
                );
                app.map.center = (
                    y_to_lat(center_world2.1, zoom2),
                    x_to_lon(center_world2.0, zoom2),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mercator_round_trips() {
        let (lat, lon, zoom) = (48.8566, 2.3522, 11.0);
        let x = lon_to_x(lon, zoom);
        let y = lat_to_y(lat, zoom);
        assert!((x_to_lon(x, zoom) - lon).abs() < 1e-9);
        assert!((y_to_lat(y, zoom) - lat).abs() < 1e-9);
    }

    #[test]
    fn a_whole_world_is_256_tiles_wide_at_zoom_8() {
        assert_eq!(lon_to_x(180.0, 8.0) - lon_to_x(-180.0, 8.0), 256.0 * 256.0);
    }
}
