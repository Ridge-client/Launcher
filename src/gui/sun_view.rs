//! "Shadows" tab: chronolocation. From the place a picture was taken, the way the camera faced,
//! where the shadows point and how long they are next to what casts them, it lists the dates and
//! times the sun stood like that, and draws the sun's path over the sky to check them by eye.
//! The picture itself is optional: it is shown to measure on.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::icons;
use super::settings_view::section;
use super::zoom::Zoom;
use super::RidgeApp;
use crate::photo::{Camera, Reading};
use crate::sun::{self, Clues, SummerTime, Window, DAY};

const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
const MAX_TEXTURE: u32 = 4096;
const ACCENT: Color32 = Color32::from_rgb(0xe8, 0xa3, 0x5a);
const OBJECT: Color32 = Color32::from_rgb(0x00, 0xac, 0xc1);
const ERROR: Color32 = Color32::from_rgb(218, 54, 51);
const NORTH: Color32 = Color32::from_rgb(0xe5, 0x39, 0x35);
const HORIZON: Color32 = Color32::from_rgb(0xb3, 0x9d, 0xdb);
/// How wide the camera's view is drawn (a phone's main camera sees about 65–75°).
const FIELD_OF_VIEW: f64 = 70.0;

/// The clues typed in the panels (the tab starts blank each time Ridge opens).
#[derive(Debug, Clone, PartialEq)]
struct Saved {
    /// "43.3026, 5.3806" or a map link.
    place: String,
    /// Compass bearing the camera faced.
    heading: f64,
    /// Where the shadows point, seen from the camera: 0° straight ahead, 90° right, ±180° back
    /// toward the camera.
    shadow_turn: f64,
    /// An object's height and its shadow's length, in the same unit.
    height: f64,
    shadow: f64,
    azimuth_tolerance: f64,
    elevation_tolerance: f64,
    /// How wide the photo sees, in degrees across its width.
    lens: f64,
    /// Standard time's offset from UTC, in hours.
    utc_offset: f64,
    summer: SummerTime,
    when: When,
    day: String,
    from: String,
    to: String,
    year: i64,
}

impl Default for Saved {
    fn default() -> Self {
        let today = today();
        Self {
            place: String::new(),
            heading: 0.0,
            shadow_turn: 0.0,
            height: 1.7,
            shadow: 1.7,
            azimuth_tolerance: 5.0,
            elevation_tolerance: 2.0,
            lens: 70.0,
            utc_offset: 1.0,
            summer: SummerTime::Europe,
            when: When::Unknown,
            day: sun::format_date(today),
            from: sun::format_date(today - 30),
            to: sun::format_date(today),
            year: sun::civil_from_days(today).0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum When {
    /// Any date: one year searched stands for every year.
    #[default]
    Unknown,
    Day,
    Between,
    Year,
}

/// The two images, side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The picture being dated, to measure an object and its shadow on.
    Photo,
    /// A map or satellite view of the place, to draw north, the camera and the shadows on.
    Map,
}

impl View {
    const BOTH: [View; 2] = [View::Photo, View::Map];

    fn index(self) -> usize {
        self as usize
    }
}

/// A line drawn on an image, from its first point to its second, in image pixels.
type Line = [[f32; 2]; 2];

/// The lines drawn on the photo, their colours and how to draw them.
const PHOTO_TOOLS: [(&str, Color32, &str); 3] = [
    ("Object", OBJECT, "Drag from the object's foot to its top"),
    (
        "Shadow",
        ACCENT,
        "Drag from the object's foot to the tip of its shadow",
    ),
    (
        "Horizon",
        HORIZON,
        "Drag to the level where lines parallel to the ground meet (the far end of a street, \
         rails or a platform). Until you draw it, the photo's middle is taken: a camera held level",
    ),
];

#[derive(Default)]
pub(super) struct State {
    saved: Saved,
    /// The clues the candidates were found for, and the candidates.
    solved: Option<(Clues, Vec<Window>)>,
    /// The candidate shown on the sky chart.
    selected: Option<usize>,
    /// Per view: the image, its texture and full size, how it is zoomed, where its canvas was
    /// on the last frame, and why it could not open.
    pictures: [Option<PathBuf>; 2],
    textures: [Option<(PathBuf, egui::TextureHandle, [f32; 2])>; 2],
    zooms: [Zoom; 2],
    canvases: [Option<Rect>; 2],
    errors: [Option<String>; 2],
    /// Lines drawn on the photo: the object (foot to top) and its shadow (foot to tip), and
    /// the row of its horizon.
    lines: [Option<Line>; 2],
    horizon: Option<f32>,
    /// The camera's view drawn on the map: from where it stood toward what it saw (north up).
    camera: Option<Line>,
    /// Which line a drag on the photo draws.
    tool: usize,
    /// A line being drawn: on which image, from where.
    drawing: Option<(View, [f32; 2])>,
}

fn today() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    now.div_euclid(DAY)
}

/// Latitude and longitude from "43.3026, 5.3806", "43.3026 N 5.3806 E" or a map link holding
/// "@43.3026,5.3806".
fn parse_place(text: &str) -> Option<(f64, f64)> {
    // in a map link, the place follows the @
    let text = text.rsplit_once('@').map_or(text, |(_, after)| after);
    let chars: Vec<char> = text.chars().collect();
    let mut numbers = Vec::new();
    let mut i = 0;
    while i < chars.len() && numbers.len() < 2 {
        let starts = |j: usize| chars.get(j).is_some_and(|c| c.is_ascii_digit());
        let c = chars[i];
        let sign = (c == '-' || c == '+') && starts(i + 1);
        if !(c.is_ascii_digit() || sign) {
            i += 1;
            continue;
        }
        let begin = i;
        i += 1;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let mut n: f64 = chars[begin..i].iter().collect::<String>().parse().ok()?;
        // a hemisphere letter after it: S and W are negative
        let mut j = i;
        while j < chars.len() && (chars[j] == ' ' || chars[j] == '°') {
            j += 1;
        }
        if let Some(h @ ('N' | 'S' | 'E' | 'W')) = chars.get(j) {
            if matches!(h, 'S' | 'W') {
                n = -n;
            }
            i = j + 1;
        }
        numbers.push(n);
    }
    let [lat, lon] = numbers[..] else {
        return None;
    };
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// The local dates to search, or why they can't be read.
fn days(s: &Saved) -> Result<(i64, i64), &'static str> {
    match s.when {
        When::Day => sun::parse_date(&s.day)
            .map(|d| (d, d))
            .ok_or("Type the date as 2026-04-15 or 15/04/2026"),
        When::Between => {
            let (Some(a), Some(b)) = (sun::parse_date(&s.from), sun::parse_date(&s.to)) else {
                return Err("Type the dates as 2026-04-15 or 15/04/2026");
            };
            if b < a {
                Err("The first date is after the last")
            } else if b - a >= sun::MAX_DAYS {
                Err("Search at most three years at once")
            } else {
                Ok((a, b))
            }
        }
        When::Year => Ok(year_days(s.year)),
        // this year stands for any other: the sun runs the same course each year, give or take
        // a day over the leap-year cycle
        When::Unknown => Ok(year_days(sun::civil_from_days(today()).0)),
    }
}

fn year_days(year: i64) -> (i64, i64) {
    (
        sun::days_from_civil(year, 1, 1),
        sun::days_from_civil(year, 12, 31),
    )
}

/// The compass-free bearing of a line on an image: degrees clockwise from the image's up.
fn image_bearing([a, b]: Line) -> f64 {
    ((b[0] - a[0]) as f64)
        .atan2(-(b[1] - a[1]) as f64)
        .to_degrees()
}

/// The camera's compass bearing, from its line on the map (north is up).
fn map_heading(st: &State) -> Option<f64> {
    st.camera.map(|l| image_bearing(l).rem_euclid(360.0))
}

/// What the lines on the photo say, once its shadow is drawn: the shadow's direction and, with
/// the object, the sun's elevation, through the photo's perspective.
fn read_photo(st: &State) -> Option<Result<Reading, &'static str>> {
    let shadow = st.lines[1]?;
    let (.., size) = st.textures[View::Photo.index()].as_ref()?;
    let camera = Camera {
        width: size[0] as f64,
        height: size[1] as f64,
        fov: st.saved.lens,
        horizon: st.horizon.unwrap_or(size[1] / 2.0) as f64,
    };
    let wide = |l: Line| l.map(|p| p.map(f64::from));
    Some(camera.read(wide(shadow), st.lines[0].map(wide)))
}

/// The shadows' compass bearing.
fn shadow_bearing(s: &Saved) -> f64 {
    (s.heading + s.shadow_turn).rem_euclid(360.0)
}

/// The clues as typed, or what is missing.
fn clues(s: &Saved) -> Result<Clues, &'static str> {
    let (lat, lon) = parse_place(&s.place).ok_or("Type where the picture was taken first")?;
    Ok(Clues {
        lat,
        lon,
        azimuth: Some((shadow_bearing(s) + 180.0).rem_euclid(360.0)),
        elevation: Some(sun::elevation_from_shadow(s.height, s.shadow)),
        azimuth_tolerance: s.azimuth_tolerance,
        elevation_tolerance: s.elevation_tolerance,
        utc_offset: (s.utc_offset * 3600.0).round() as i64,
        summer: s.summer,
        days: days(s)?,
    })
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.shadows;
    // an image dropped on the window opens where it was dropped (the photo when unclear)
    let dropped: Option<(PathBuf, Option<Pos2>)> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .find(|p| !p.as_os_str().is_empty())
            .map(|p| (p, i.pointer.latest_pos()))
    });
    if let Some((file, at)) = dropped {
        let on_map =
            at.is_some_and(|p| st.canvases[View::Map.index()].is_some_and(|r| r.contains(p)));
        open_picture(st, if on_map { View::Map } else { View::Photo }, file);
    }

    // the camera line on the map sets its bearing; the lines on the photo where the shadows
    // point from it and, with the object, the lengths (in camera heights)
    let heading = map_heading(st);
    if let Some(h) = heading {
        st.saved.heading = h;
    }
    let photo = read_photo(st).and_then(Result::ok);
    if let Some(r) = photo {
        st.saved.shadow_turn = r.turn;
        if let Some(h) = r.height {
            st.saved.height = h;
            st.saved.shadow = r.length;
        }
    }
    let from_photo = FromPhoto {
        direction: photo.is_some(),
        lengths: photo.is_some_and(|r| r.height.is_some()),
    };

    // find the candidates again when a clue changed (before the panel, whose chart shows them)
    let clues = clues(&st.saved);
    match &clues {
        Ok(c) if st.solved.as_ref().is_none_or(|(old, _)| old != c) => {
            let mut windows = sun::windows(&c.solve());
            if st.saved.when == When::Unknown {
                windows = sun::wrap_year(windows, c.days);
            }
            st.solved = Some((c.clone(), windows));
            st.selected = None;
        }
        Err(_) => st.solved = None,
        _ => {}
    }

    egui::Panel::right("shadow_clues")
        .resizable(false)
        .exact_size(360.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("shadow_clues")
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    section(ui, "Sun's path", |ui| sky_chart(st, &clues, ui));
                    ui.add_space(6.0);
                    clues_panel(&mut st.saved, heading.is_some(), from_photo, ui);
                });
        });

    // the photo and the map side by side on top, always the same size; the place under them;
    // then the candidates
    ui.add_space(6.0);
    let full = ui.available_rect_before_wrap();
    // clear of the window's edge, the panel's border and the bottom
    let area = Rect::from_min_max(
        full.min + Vec2::new(6.0, 0.0),
        full.max - Vec2::new(12.0, 10.0),
    );
    let gap = 10.0;
    let top_h = (area.height() * 0.45).round();
    // the photo and the map get more room between them than the cards below do
    let side_gap = 32.0;
    let half = (area.width() - side_gap) / 2.0;
    for (i, view) in View::BOTH.into_iter().enumerate() {
        let left = area.min.x + i as f32 * (half + side_gap);
        let pane = Rect::from_min_size(Pos2::new(left, area.min.y), Vec2::new(half, top_h));
        // a header of fixed height, so both images start level whatever it holds
        let (header, image) = pane.split_top_bottom_at_y(pane.top() + 50.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(header), |ui| {
            pane_header(st, view, ui)
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(image), |ui| {
            canvas(st, view, ui)
        });
    }
    let place = Rect::from_min_size(
        Pos2::new(area.min.x, area.min.y + top_h + gap),
        Vec2::new(area.width(), 108.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(place), |ui| {
        place_card(&mut st.saved, ui)
    });
    let list_rect = Rect::from_min_max(Pos2::new(area.min.x, place.bottom() + gap), area.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
        // a card of its own, the height of the space left
        egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                candidates(st, &clues, ui)
            });
    });
    ui.allocate_rect(full, egui::Sense::hover());
    // a clue changed after the candidates were found: find them again on the next frame
    if st.solved.as_ref().map(|(c, _)| c) != self::clues(&st.saved).ok().as_ref() {
        ui.ctx().request_repaint();
    }
}

/// Over each image: its name, open and close, the lines to draw, and the zoom.
fn pane_header(st: &mut State, view: View, ui: &mut egui::Ui) {
    let i = view.index();
    let loaded = st.pictures[i].is_some();
    ui.horizontal(|ui| {
        let (title, hint) = match view {
            View::Photo => ("Photo", "The picture being dated: an object and its shadow"),
            View::Map => (
                "Map",
                "A map or satellite view, north up: the camera's view",
            ),
        };
        ui.label(egui::RichText::new(title).strong().size(15.0))
            .on_hover_text(hint);
        let open = if loaded {
            ui.button(icons::with(icons::FOLDER_OPEN, "Import…"))
                .on_hover_text("Open another image (or drop one here)")
        } else {
            ui.button(icons::with(icons::FOLDER_OPEN, "Import…"))
                .on_hover_text(match view {
                    View::Photo => "The picture, to measure the shadow on (or drop it here)",
                    View::Map => "A map or satellite screenshot of the place (or drop it here)",
                })
        };
        if open.clicked() {
            choose_picture(st, view);
        }
        if !loaded {
            return;
        }
        if ui
            .small_button(icons::CANCEL)
            .on_hover_text("Close this image")
            .clicked()
        {
            close_picture(st, view);
            return;
        }
        ui.separator();
        match view {
            View::Photo => {
                for (t, (label, colour, hint)) in PHOTO_TOOLS.iter().enumerate() {
                    let text = egui::RichText::new(*label).color(if st.tool == t {
                        *colour
                    } else {
                        ui.visuals().text_color()
                    });
                    if ui
                        .add(egui::Button::new(text).small().selected(st.tool == t))
                        .on_hover_text(*hint)
                        .clicked()
                    {
                        st.tool = t;
                    }
                }
                if (st.lines.iter().any(Option::is_some) || st.horizon.is_some())
                    && ui
                        .small_button(icons::CLEAN)
                        .on_hover_text("Remove the lines drawn on the photo (then type the clues)")
                        .clicked()
                {
                    st.lines = [None; 2];
                    st.horizon = None;
                }
            }
            View::Map => {
                if st.camera.is_some()
                    && ui
                        .small_button(icons::CLEAN)
                        .on_hover_text("Remove the camera line (then type its bearing)")
                        .clicked()
                {
                    st.camera = None;
                }
            }
        }
        // the zoom at the header's right end, over the image's right edge
        if let (Some((.., size)), Some(canvas)) = (&st.textures[i], st.canvases[i]) {
            let size = *size;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                st.zooms[i].buttons(ui, canvas, size, true);
            });
        }
    });
    // what the lines say, or how to draw them
    let status = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate());
    };
    if let Some(e) = &st.errors[i] {
        ui.add(egui::Label::new(egui::RichText::new(e).color(ERROR)).truncate());
        return;
    }
    let text = match view {
        View::Photo => match read_photo(st) {
            Some(Ok(r)) => {
                let sun = r
                    .elevation
                    .map_or(" · draw the object for the sun's height".into(), |e| {
                        format!(" · sun {e:.1}° high")
                    });
                format!(
                    "Shadows point {:+.0}° ({}){sun}{}",
                    r.turn,
                    turn_words(r.turn),
                    if st.horizon.is_none() { " · horizon taken in the middle" } else { "" }
                )
            }
            Some(Err(e)) => {
                ui.add(egui::Label::new(egui::RichText::new(e).color(ERROR)).truncate());
                return;
            }
            None if loaded => {
                "Drag the shadow from the object's foot to its tip, the object from its foot to its top, and the horizon".into()
            }
            None => "The photo: an object, its shadow and the horizon".into(),
        },
        View::Map => match map_heading(st) {
            Some(h) => format!("Camera faces {h:.0}° ({})", sun::compass(h)),
            None if loaded => {
                "North is up · drag from where the camera stood toward what it saw".into()
            }
            None => "Optional: a map with north up, to draw the camera's view".into(),
        },
    };
    status(ui, &text);
}

fn is_picture(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| PICTURES.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn open_picture(st: &mut State, view: View, file: PathBuf) {
    let i = view.index();
    if !is_picture(&file) {
        st.errors[i] = Some(format!(
            "{} is not a picture Ridge can open (PNG, JPG, WebP, GIF)",
            file.display()
        ));
        return;
    }
    close_picture(st, view);
    st.pictures[i] = Some(file);
}

fn close_picture(st: &mut State, view: View) {
    let i = view.index();
    st.pictures[i] = None;
    match view {
        View::Photo => {
            st.lines = [None; 2];
            st.horizon = None;
        }
        View::Map => st.camera = None,
    }
    st.zooms[i] = Zoom::default();
    st.errors[i] = None;
    st.drawing = None;
}

fn choose_picture(st: &mut State, view: View) {
    let title = match view {
        View::Photo => "Choose the photo",
        View::Map => "Choose a map or satellite screenshot",
    };
    if let Some(file) = rfd::FileDialog::new()
        .set_title(title)
        .add_filter("Pictures", &PICTURES)
        .pick_file()
    {
        open_picture(st, view, file);
    }
}

// ---- the clues ----

/// Where and when to look, and how sure the clues are, in two rows under the images.
fn place_card(s: &mut Saved, ui: &mut egui::Ui) {
    section(ui, "Place and date", |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut s.place)
                    .desired_width(300.0)
                    .hint_text("43.3026, 5.3806 or a map link"),
            )
            .on_hover_text("Latitude and longitude, as Google Maps or OpenStreetMap gives them");
            match parse_place(&s.place) {
                Some((lat, lon)) => {
                    ui.weak(format!(
                        "{:.5}° {}, {:.5}° {}",
                        lat.abs(),
                        if lat < 0.0 { 'S' } else { 'N' },
                        lon.abs(),
                        if lon < 0.0 { 'W' } else { 'E' }
                    ));
                    if ui
                        .small_button(icons::with(icons::OPEN, "Map"))
                        .on_hover_text("Check the place on OpenStreetMap")
                        .clicked()
                    {
                        let _ = open::that(format!(
                            "https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=18/{lat}/{lon}"
                        ));
                    }
                }
                None if s.place.trim().is_empty() => {
                    ui.weak("Latitude and longitude");
                }
                None => {
                    ui.colored_label(ERROR, "Not a latitude and longitude");
                }
            }
            ui.separator();
            ui.label("Clock: UTC");
            ui.add(
                egui::DragValue::new(&mut s.utc_offset)
                    .range(-12.0..=14.0)
                    .speed(0.05)
                    .fixed_decimals(1)
                    .custom_formatter(|v, _| sun::format_offset((v * 3600.0).round() as i64)),
            )
            .on_hover_text(
                "Standard (winter) time there: +1 for France, 0 for the UK, -5 for New York",
            );
            egui::ComboBox::from_id_salt("summer_time")
                .width(ui.available_width().clamp(120.0, 300.0))
                .selected_text(s.summer.label())
                .show_ui(ui, |ui| {
                    for rule in SummerTime::ALL {
                        ui.selectable_value(&mut s.summer, rule, rule.label());
                    }
                });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Dates");
            ui.radio_value(&mut s.when, When::Unknown, "Unknown")
                .on_hover_text("Any date: they come without a year, good to a day or so");
            ui.radio_value(&mut s.when, When::Year, "A year");
            ui.radio_value(&mut s.when, When::Between, "Between");
            ui.radio_value(&mut s.when, When::Day, "One day");
            let date = |ui: &mut egui::Ui, text: &mut String| {
                let ok = sun::parse_date(text).is_some();
                let mut edit = egui::TextEdit::singleline(text)
                    .desired_width(100.0)
                    .hint_text("2026-04-15");
                if !ok {
                    edit = edit.text_color(ERROR);
                }
                ui.add(edit)
                    .on_hover_text("2026-04-15 or 15/04/2026");
            };
            match s.when {
                When::Unknown => {}
                When::Year => {
                    ui.add(
                        egui::DragValue::new(&mut s.year)
                            .range(1800..=2200)
                            .speed(0.1),
                    );
                }
                When::Between => {
                    date(ui, &mut s.from);
                    ui.label("to");
                    date(ui, &mut s.to);
                }
                When::Day => date(ui, &mut s.day),
            }
            if let Err(e) = days(s) {
                ui.colored_label(ERROR, e);
            }
            ui.separator();
            ui.label("How sure: direction ±")
                .on_hover_text("Each degree of the sun's direction is about 4 minutes of the day. Widen these if nothing fits");
            ui.add(
                egui::DragValue::new(&mut s.azimuth_tolerance)
                    .range(0.5..=45.0)
                    .speed(0.1)
                    .suffix("°"),
            );
            ui.label("elevation ±");
            ui.add(
                egui::DragValue::new(&mut s.elevation_tolerance)
                    .range(0.2..=30.0)
                    .speed(0.1)
                    .suffix("°"),
            );
        });
    });
}

/// Which clues the lines on the photo give.
#[derive(Clone, Copy)]
struct FromPhoto {
    direction: bool,
    lengths: bool,
}

/// `heading_on_map`, `photo`: those clues come from lines drawn on the images.
fn clues_panel(s: &mut Saved, heading_on_map: bool, photo: FromPhoto, ui: &mut egui::Ui) {
    section(ui, "Shadow direction", |ui| {
        egui::Grid::new("shadow_direction")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Camera faces");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!heading_on_map, |ui| {
                        degrees(ui, &mut s.heading, 0.0..=359.9).on_hover_text(
                            "Compass bearing of the view: line up something in the picture on a map",
                        )
                    })
                    .response
                    .on_disabled_hover_text("Set by the camera line on the map");
                    ui.weak(sun::compass(s.heading));
                });
                ui.end_row();
                ui.label("Shadows point");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!photo.direction, |ui| {
                        degrees(ui, &mut s.shadow_turn, -180.0..=180.0).on_hover_text(
                            "Seen from the camera: 0° straight ahead, 90° right, -90° left, ±180° toward the camera",
                        )
                    })
                    .response
                    .on_disabled_hover_text("Set by the shadow line on the photo");
                    ui.weak(turn_words(s.shadow_turn));
                });
                ui.end_row();
                ui.label("So, on a map");
                ui.horizontal(|ui| {
                    let mut bearing = shadow_bearing(s);
                    let changed = ui
                        .add_enabled_ui(!photo.direction, |ui| {
                            degrees(ui, &mut bearing, 0.0..=359.9)
                                .on_hover_text(
                                    "The shadows' compass bearing, if you measured it on a map",
                                )
                                .changed()
                        })
                        .inner;
                    if changed {
                        s.shadow_turn = sun::angle_diff(s.heading, bearing);
                    }
                    ui.weak(sun::compass(bearing));
                });
                ui.end_row();
                ui.label("Photo's view");
                ui.add(
                    egui::DragValue::new(&mut s.lens)
                        .range(20.0..=130.0)
                        .speed(0.2)
                        .max_decimals(0)
                        .suffix("°"),
                )
                .on_hover_text(
                    "How wide the photo sees across its width: a phone's main camera ≈ 65–75°, its 0.5× ultra-wide ≈ 110°, its 2× ≈ 40°. Uncropped photos only",
                );
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.weak(match (heading_on_map, photo.direction) {
            (true, true) => "From the camera line on the map and the shadow on the photo: clear them there to type.",
            (true, false) => "The camera comes from its line on the map; draw a shadow on the photo for where the shadows point.",
            (false, true) => "The shadows come from the photo; draw the camera's view on the map for its bearing.",
            (false, false) => "Or draw the camera's view on the map and a shadow on the photo.",
        });
        ui.vertical_centered(|ui| dial(s, photo.direction, ui));
        let sun_bearing = (shadow_bearing(s) + 180.0).rem_euclid(360.0);
        ui.weak(format!(
            "The sun was at {sun_bearing:.0}° ({}), behind the objects.",
            sun::compass(sun_bearing)
        ));
    });
    ui.add_space(6.0);

    section(ui, "Shadow length", |ui| {
        egui::Grid::new("shadow_length")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Object height");
                ui.add_enabled(
                    !photo.lengths,
                    egui::DragValue::new(&mut s.height)
                        .range(0.01..=10_000.0)
                        .speed(0.01)
                        .max_decimals(2),
                )
                .on_hover_text(
                    "A person ≈ 1.7 m, a door ≈ 2 m, a storey ≈ 3 m, a lamp post ≈ 4–10 m",
                )
                .on_disabled_hover_text("Set by the lines on the photo, in camera heights");
                ui.end_row();
                ui.label("Its shadow");
                ui.add_enabled(
                    !photo.lengths,
                    egui::DragValue::new(&mut s.shadow)
                        .range(0.01..=100_000.0)
                        .speed(0.01)
                        .max_decimals(2),
                )
                .on_hover_text("From the object's foot to the tip of its shadow, in the same unit")
                .on_disabled_hover_text("Set by the lines on the photo, in camera heights");
                ui.end_row();
            });
        let elevation = sun::elevation_from_shadow(s.height, s.shadow);
        ui.label(format!(
            "Sun {elevation:.1}° above the horizon (shadow {:.2}× the height)",
            s.shadow / s.height
        ));
        ui.weak(if photo.lengths {
            "From the object and its shadow on the photo, put back on the ground through its perspective (in heights of the camera). Clear them there to type."
        } else {
            "On level ground, in the same unit. Paving, tiles and markings help measure a shadow that runs toward the camera."
        });
    });
    ui.add_space(6.0);
}

/// A bearing in degrees.
fn degrees(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> egui::Response {
    ui.add(
        egui::DragValue::new(value)
            .range(range)
            .speed(0.5)
            .max_decimals(1)
            .suffix("°"),
    )
}

/// "35° left, toward the camera"
fn turn_words(turn: f64) -> String {
    let side = turn.abs();
    if side < 3.0 {
        return "straight away".into();
    }
    if side > 177.0 {
        return "toward the camera".into();
    }
    let lr = if turn > 0.0 { "right" } else { "left" };
    if (side - 90.0).abs() < 3.0 {
        lr.into()
    } else if side < 90.0 {
        format!("away, {lr}")
    } else {
        format!("toward, {lr}")
    }
}

/// Screen direction (y down) of a bearing turned `turn` degrees clockwise from straight up.
fn dir(turn: f64) -> Vec2 {
    let r = turn.to_radians() as f32;
    Vec2::new(r.sin(), -r.cos())
}

/// The camera seen from above, looking up: drag around it to point the shadows. The compass
/// letters turn with the camera.
fn dial(s: &mut Saved, locked: bool, ui: &mut egui::Ui) {
    let size = 190.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click_and_drag());
    let response = if locked {
        response
    } else {
        response.on_hover_cursor(egui::CursorIcon::Crosshair)
    };
    let c = rect.center();
    let r = size / 2.0 - 16.0;
    if let Some(p) = response.interact_pointer_pos().filter(|_| !locked) {
        if response.dragged() || response.clicked() {
            let v = p - c;
            if v.length() > 4.0 {
                s.shadow_turn = (v.x as f64).atan2(-v.y as f64).to_degrees().round();
            }
        }
    }
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.circle_filled(c, r, visuals.extreme_bg_color);
    painter.circle_stroke(c, r, visuals.widgets.noninteractive.bg_stroke);
    // the compass around it, turned so the camera looks up
    for (bearing, letter) in [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")] {
        let d = dir(bearing - s.heading);
        painter.text(
            c + d * (r + 9.0),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(12.0),
            if letter == "N" {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
        painter.line_segment(
            [c + d * (r - 5.0), c + d * r],
            visuals.widgets.noninteractive.fg_stroke,
        );
    }
    // what the camera sees
    let half = FIELD_OF_VIEW / 2.0;
    let fov: Vec<Pos2> = std::iter::once(c)
        .chain((0..=12).map(|i| c + dir(-half + FIELD_OF_VIEW * i as f64 / 12.0) * (r - 2.0)))
        .collect();
    painter.add(egui::Shape::convex_polygon(
        fov,
        Color32::from_white_alpha(14),
        Stroke::NONE,
    ));
    painter.text(
        c + Vec2::new(0.0, -r * 0.55),
        egui::Align2::CENTER_CENTER,
        "view",
        egui::FontId::proportional(11.0),
        visuals.weak_text_color(),
    );
    // the camera
    let cam = [
        c + Vec2::new(0.0, -7.0),
        c + Vec2::new(-6.0, 6.0),
        c + Vec2::new(6.0, 6.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        cam.to_vec(),
        visuals.strong_text_color(),
        Stroke::NONE,
    ));
    // the shadow, and the sun opposite it
    let d = dir(s.shadow_turn);
    let tip = c + d * (r - 10.0);
    let stroke = Stroke::new(3.0, ACCENT);
    painter.line_segment([c + d * 10.0, tip], stroke);
    let wing = |a: f64| tip + dir(s.shadow_turn + a) * 9.0;
    painter.line_segment([tip, wing(150.0)], stroke);
    painter.line_segment([tip, wing(-150.0)], stroke);
    let sun_at = c - d * (r - 10.0);
    painter.circle_filled(sun_at, 6.0, Color32::from_rgb(0xfd, 0xd8, 0x35));
    for i in 0..8 {
        let ray = dir(i as f64 * 45.0);
        painter.line_segment(
            [sun_at + ray * 8.0, sun_at + ray * 11.0],
            Stroke::new(1.5, Color32::from_rgb(0xfd, 0xd8, 0x35)),
        );
    }
    response.on_hover_text(if locked {
        "Set by the shadow line on the photo"
    } else {
        "Drag around the camera to point the shadows as they fall in the picture"
    });
}

// ---- the photo and the map ----

fn texture(st: &mut State, view: View, ctx: &egui::Context) -> Option<(egui::TextureId, [f32; 2])> {
    let i = view.index();
    let path = st.pictures[i].clone()?;
    if st.textures[i].as_ref().is_none_or(|(p, ..)| *p != path) {
        st.textures[i] = None;
        let picture = match image::open(&path) {
            Ok(p) => p,
            Err(e) => {
                close_picture(st, view);
                st.errors[i] = Some(format!("Couldn't open the image: {e}"));
                return None;
            }
        };
        let size = [picture.width() as f32, picture.height() as f32];
        let shown = if picture.width().max(picture.height()) > MAX_TEXTURE {
            picture.thumbnail(MAX_TEXTURE, MAX_TEXTURE)
        } else {
            picture
        };
        let rgba = shown.to_rgba8();
        let pixels = [rgba.width() as usize, rgba.height() as usize];
        let colour = egui::ColorImage::from_rgba_unmultiplied(pixels, rgba.as_raw());
        let handle = ctx.load_texture(
            path.display().to_string(),
            colour,
            egui::TextureOptions::LINEAR,
        );
        st.textures[i] = Some((path, handle, size));
    }
    st.textures[i].as_ref().map(|(_, t, size)| (t.id(), *size))
}

/// The map's north, up: a round badge with an N and a red point toward north, as map apps have.
fn compass_badge(painter: &egui::Painter, ui: &egui::Ui, area: Rect) {
    let c = Pos2::new(area.left() + 26.0, area.top() + 28.0);
    let r = 15.0;
    painter.circle(
        c,
        r,
        Color32::from_black_alpha(185),
        Stroke::new(1.0, Color32::from_white_alpha(45)),
    );
    // the point on the rim, toward north
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(0.0, -r - 6.0),
            c + Vec2::new(-5.0, -r + 1.5),
            c + Vec2::new(5.0, -r + 1.5),
        ],
        NORTH,
        Stroke::NONE,
    ));
    painter.text(
        c + Vec2::new(0.0, 1.0),
        egui::Align2::CENTER_CENTER,
        "N",
        egui::FontId::proportional(14.0),
        Color32::WHITE,
    );
    let badge = Rect::from_center_size(c, Vec2::splat(2.0 * r + 8.0));
    ui.interact(badge, ui.scope_id().with("map_north"), egui::Sense::hover())
        .on_hover_text("North is up: use a map or satellite view with north at the top");
}

/// An arrow from `a` to `b`, outlined so it shows on any image.
fn arrow(painter: &egui::Painter, a: Pos2, b: Pos2, colour: Color32) {
    let d = (b - a).normalized();
    let wings = [d.rot90(), -d.rot90()].map(|n| b - d * 12.0 + n * 6.0);
    for (width, c) in [(4.5, Color32::from_black_alpha(150)), (2.0, colour)] {
        let stroke = Stroke::new(width, c);
        painter.line_segment([a, b], stroke);
        painter.line_segment([b, wings[0]], stroke);
        painter.line_segment([b, wings[1]], stroke);
    }
    painter.circle_filled(a, 4.0, colour);
}

fn canvas(st: &mut State, view: View, ui: &mut egui::Ui) {
    let i = view.index();
    let area = ui.available_rect_before_wrap();
    st.canvases[i] = Some(area);
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 6.0, ui.visuals().extreme_bg_color);
    let file_over = ui.input(|inp| !inp.raw.hovered_files.is_empty())
        && ui
            .input(|inp| inp.pointer.latest_pos())
            .is_some_and(|p| area.contains(p));
    if st.pictures[i].is_none() {
        // blank until an image is opened; outlined while a file is dragged over it
        if file_over {
            painter.rect_stroke(
                area,
                6.0,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            choose_picture(st, view);
        }
        return;
    }
    let Some((tex, size)) = texture(st, view, ui.ctx()) else {
        return;
    };
    st.zooms[i].interact(ui, &response, area, size);
    let (shown, scale) = st.zooms[i].place(area, size);
    painter.image(
        tex,
        shown,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    if file_over {
        painter.rect_stroke(
            area,
            6.0,
            Stroke::new(2.0, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let to_screen = |p: [f32; 2]| shown.min + Vec2::new(p[0], p[1]) * scale;
    let to_picture = |p: Pos2| {
        let q = (p - shown.min) / scale;
        [q.x.clamp(0.0, size[0]), q.y.clamp(0.0, size[1])]
    };
    let pointer = ui.input(|inp| inp.pointer.interact_pos());
    if pointer.is_some_and(|p| shown.contains(p) && area.contains(p)) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        st.drawing = pointer
            .filter(|p| shown.contains(*p))
            .map(|p| (view, to_picture(p)));
    }
    if let (Some((on, start)), Some(p)) = (st.drawing, pointer) {
        let end = to_picture(p);
        if on == view && view == View::Photo && st.tool == 2 {
            // the horizon follows the pointer's height
            st.horizon = Some(end[1]);
        } else if on == view && (to_screen(end) - to_screen(start)).length() > 6.0 {
            let line = Some([start, end]);
            match view {
                View::Photo => st.lines[st.tool] = line,
                View::Map => st.camera = line,
            }
        }
    }
    if response.drag_stopped() {
        st.drawing = None;
    }

    if view == View::Photo {
        // the horizon across the photo: drawn, or its middle until it is
        let y = to_screen([0.0, st.horizon.unwrap_or(size[1] / 2.0)]).y;
        let (colour, text) = match st.horizon {
            Some(_) => (HORIZON, "Horizon"),
            None => (Color32::from_white_alpha(90), "Horizon (middle)"),
        };
        painter.add(egui::Shape::dashed_line(
            &[Pos2::new(shown.left(), y), Pos2::new(shown.right(), y)],
            Stroke::new(1.5, colour),
            8.0,
            6.0,
        ));
        painter.text(
            Pos2::new(shown.left().max(area.left()) + 6.0, y - 3.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(12.0),
            colour,
        );
        for (line, colour) in st.lines.iter().zip([OBJECT, ACCENT]) {
            let Some([a, b]) = line else {
                continue;
            };
            let (a, b) = (to_screen(*a), to_screen(*b));
            painter.line_segment([a, b], Stroke::new(4.0, Color32::from_black_alpha(140)));
            painter.line_segment([a, b], Stroke::new(2.0, colour));
            painter.circle_filled(a, 4.0, colour);
            painter.circle_stroke(b, 4.0, Stroke::new(2.0, colour));
        }
        return;
    }

    let label = |at: Pos2, text: &str, colour: Color32| {
        let galley =
            painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), colour);
        let r = Rect::from_center_size(at, galley.size() + Vec2::new(8.0, 4.0));
        painter.rect_filled(r, 3.0, Color32::from_black_alpha(170));
        painter.galley(r.min + Vec2::new(4.0, 2.0), galley, colour);
    };
    // the camera: where it stood and what it saw
    if let Some([a, b]) = st.camera {
        let (a, b) = (to_screen(a), to_screen(b));
        let reach = (b - a).length();
        let ahead = image_bearing([[a.x, a.y], [b.x, b.y]]);
        let half = FIELD_OF_VIEW / 2.0;
        let fov: Vec<Pos2> = std::iter::once(a)
            .chain(
                (0..=12).map(|i| a + dir(ahead - half + FIELD_OF_VIEW * i as f64 / 12.0) * reach),
            )
            .collect();
        painter.add(egui::Shape::convex_polygon(
            fov,
            Color32::from_rgba_unmultiplied(0x00, 0xac, 0xc1, 40),
            Stroke::new(1.0, OBJECT),
        ));
        arrow(&painter, a, b, OBJECT);
        if let Some(heading) = map_heading(st) {
            label(
                a - (b - a).normalized() * 18.0,
                &format!("{heading:.0}° {}", sun::compass(heading)),
                OBJECT,
            );
        }
    }
    // north is always up: a compass badge in the corner, over everything
    compass_badge(&painter, ui, area);
}

// ---- the sky ----

/// The sky seen from below, north up: the middle is overhead, the rim the horizon. The sun's
/// path on the chosen day, where the clues put it, and the camera's view.
fn sky_chart(st: &mut State, clues: &Result<Clues, &'static str>, ui: &mut egui::Ui) {
    let side = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let visuals = ui.visuals();
    let c = rect.center();
    let r = side / 2.0 - 22.0;
    if r < 40.0 {
        return;
    }
    let at = |azimuth: f64, elevation: f64| {
        c + dir(azimuth) * (r * (90.0 - elevation.clamp(0.0, 90.0)) as f32 / 90.0)
    };
    painter.circle_filled(c, r, visuals.extreme_bg_color);
    let faint = Stroke::new(1.0, Color32::from_white_alpha(22));
    for el in [30.0, 60.0] {
        painter.circle_stroke(c, r * (90.0 - el) / 90.0, faint);
        painter.text(
            at(180.0, el as f64) + Vec2::new(3.0, 0.0),
            egui::Align2::LEFT_TOP,
            format!("{el:.0}°"),
            egui::FontId::proportional(10.0),
            visuals.weak_text_color(),
        );
    }
    painter.circle_stroke(c, r, visuals.widgets.noninteractive.bg_stroke);
    for (bearing, letter) in [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")] {
        painter.line_segment([c + dir(bearing) * (r - 6.0), c + dir(bearing) * r], faint);
        painter.text(
            c + dir(bearing) * (r + 11.0),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(13.0),
            if letter == "N" {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
    }
    let Ok(clues) = clues else {
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            "The sky over the place",
            egui::FontId::proportional(13.0),
            visuals.weak_text_color(),
        );
        return;
    };
    let s = &st.saved;
    // the camera's view
    {
        let half = FIELD_OF_VIEW / 2.0;
        let fov: Vec<Pos2> = std::iter::once(c)
            .chain(
                (0..=16).map(|i| c + dir(s.heading - half + FIELD_OF_VIEW * i as f64 / 16.0) * r),
            )
            .collect();
        painter.add(egui::Shape::convex_polygon(
            fov,
            Color32::from_white_alpha(8),
            Stroke::NONE,
        ));
        painter.text(
            c + dir(s.heading) * (r * 0.82),
            egui::Align2::CENTER_CENTER,
            "camera",
            egui::FontId::proportional(11.0),
            visuals.weak_text_color(),
        );
    }
    // where the clues put the sun: a line toward it, a ring at its height
    let target = Color32::from_rgba_unmultiplied(0xfd, 0xd8, 0x35, 40);
    if let Some(az) = clues.azimuth {
        let tol = clues.azimuth_tolerance;
        let wedge: Vec<Pos2> = std::iter::once(c)
            .chain((0..=8).map(|i| c + dir(az - tol + 2.0 * tol * i as f64 / 8.0) * r))
            .collect();
        painter.add(egui::Shape::convex_polygon(wedge, target, Stroke::NONE));
        painter.line_segment(
            [c, c + dir(az) * r],
            Stroke::new(1.0, Color32::from_rgb(0xfd, 0xd8, 0x35)),
        );
    }
    if let Some(el) = clues.elevation {
        let tol = clues.elevation_tolerance;
        let (outer, inner) = (
            r * (90.0 - (el - tol).max(0.0)) as f32 / 90.0,
            r * (90.0 - (el + tol).min(90.0)) as f32 / 90.0,
        );
        painter.circle_stroke(
            c,
            (outer + inner) / 2.0,
            Stroke::new((outer - inner).max(1.0), target),
        );
        painter.circle_stroke(
            c,
            r * (90.0 - el) as f32 / 90.0,
            Stroke::new(1.0, Color32::from_rgb(0xfd, 0xd8, 0x35)),
        );
    }

    // the sun's path on the chosen candidate's day (or the first day searched)
    let windows = st.solved.as_ref().map_or(&[][..], |(_, w)| &w[..]);
    let best = st
        .selected
        .and_then(|i| windows.get(i))
        .or(windows.first())
        .map(|w| w.best);
    let day = best.map_or(clues.days.0, |m| m.local_day());
    let offset = |unix: i64| sun::utc_offset(unix, clues.utc_offset, clues.summer);
    let start = day * DAY - offset(day * DAY + DAY / 2 - clues.utc_offset);
    let mut path: Vec<Pos2> = Vec::new();
    let path_stroke = Stroke::new(2.0, Color32::from_rgb(0x1e, 0x88, 0xe5));
    let flush = |path: &mut Vec<Pos2>| {
        if path.len() > 1 {
            painter.add(egui::Shape::line(std::mem::take(path), path_stroke));
        }
        path.clear();
    };
    for minute in (0..=24 * 60).step_by(5) {
        let unix = start + minute * 60;
        let p = sun::position(unix as f64, clues.lat, clues.lon);
        if p.elevation > 0.0 {
            path.push(at(p.azimuth, p.elevation));
        } else {
            flush(&mut path);
        }
        // the local hours along it
        if minute % 60 == 0 && p.elevation > 0.0 {
            let q = at(p.azimuth, p.elevation);
            painter.circle_filled(q, 2.5, path_stroke.color);
            painter.text(
                q + (q - c).normalized() * 10.0,
                egui::Align2::CENTER_CENTER,
                format!("{}h", minute / 60),
                egui::FontId::proportional(10.0),
                visuals.weak_text_color(),
            );
        }
    }
    flush(&mut path);
    if let Some(m) = best {
        let q = at(m.sun.azimuth, m.sun.elevation);
        painter.circle_filled(q, 7.0, ACCENT);
        painter.circle_stroke(q, 7.0, Stroke::new(1.5, Color32::BLACK));
    }
    painter.text(
        rect.left_bottom() + Vec2::new(2.0, 0.0),
        egui::Align2::LEFT_BOTTOM,
        format!("Sun's path on {}", date(day, s.when == When::Unknown)),
        egui::FontId::proportional(11.0),
        visuals.weak_text_color(),
    );
}

// ---- the candidates ----

/// "18 Apr 2026", or "18 Apr" when the year is unknown.
fn date(days: i64, any_year: bool) -> String {
    let long = sun::format_date_long(days);
    if any_year {
        long.rsplit_once(' ')
            .map_or(long.clone(), |(d, _)| d.to_string())
    } else {
        long
    }
}

/// "18 Apr 2026" or "12 – 24 Apr 2026" or "29 Mar 2026 – 2 Apr 2026" (without the years when
/// the year is unknown).
fn date_range(first: i64, last: i64, any_year: bool) -> String {
    if first == last {
        return date(first, any_year);
    }
    let (y1, m1, d1) = sun::civil_from_days(first);
    let (y2, m2, _) = sun::civil_from_days(last);
    if (y1, m1) == (y2, m2) {
        format!("{d1} – {}", date(last, any_year))
    } else {
        format!("{} – {}", date(first, any_year), date(last, any_year))
    }
}

fn clock_range(a: i64, b: i64) -> String {
    if a / 60 == b / 60 {
        sun::format_clock(a)
    } else {
        format!("{}–{}", sun::format_clock(a), sun::format_clock(b))
    }
}

/// A candidate in words, for the list and the clipboard.
fn describe(w: &Window, any_year: bool) -> [String; 3] {
    let m = w.best;
    let days = w.last_day - w.first_day + 1;
    let dates = if days > 1 {
        format!(
            "{} · {days} days",
            date_range(w.first_day, w.last_day, any_year)
        )
    } else {
        date_range(w.first_day, w.last_day, any_year)
    };
    let local = format!(
        "{} local (UTC{})",
        clock_range(w.earliest, w.latest),
        sun::format_offset(m.offset)
    );
    let utc = |clock: i64| clock - m.offset;
    let times = format!(
        "{local} · {} UTC",
        clock_range(utc(w.earliest), utc(w.latest))
    );
    let best = format!(
        "Closest: {} {} · sun {:.1}° ({}), {:.1}° high",
        date(m.local_day(), any_year),
        sun::format_clock(m.local_clock()),
        m.sun.azimuth,
        sun::compass(m.sun.azimuth),
        m.sun.elevation,
    );
    [dates, times, best]
}

fn candidates(st: &mut State, clues: &Result<Clues, &'static str>, ui: &mut egui::Ui) {
    let clues = match clues {
        Ok(c) => c,
        Err(e) => {
            ui.label(egui::RichText::new("Candidates").strong().size(16.0));
            ui.weak(*e);
            return;
        }
    };
    let windows = st.solved.as_ref().map_or(&[][..], |(_, w)| &w[..]);
    let any_year = st.saved.when == When::Unknown;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Candidates").strong().size(16.0));
        ui.weak(windows.len().to_string());
        if !windows.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(icons::with(icons::COPY, "Copy"))
                    .on_hover_text("Copy the clues and the candidates, for a report")
                    .clicked()
                {
                    ui.ctx().copy_text(report(&st.saved, clues, windows));
                }
            });
        }
    });
    if windows.is_empty() {
        ui.weak("Nothing fits. Check the camera's bearing and the shadows' direction, widen \"How sure\", or search more dates.");
        return;
    }
    let single = clues.days.0 == clues.days.1;
    if !single && windows.len() > 1 {
        ui.weak("The sun stands the same way twice a year, either side of the June solstice: pick with the season in the picture.");
    }
    ui.add_space(4.0);
    let selected = st.selected.unwrap_or(0);
    egui::ScrollArea::vertical()
        .id_salt("shadow_candidates")
        .auto_shrink(false)
        .show(ui, |ui| {
            for (i, w) in windows.iter().enumerate() {
                let [dates, times, best] = describe(w, any_year);
                let frame = egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(if i == selected {
                        Stroke::new(1.5, ACCENT)
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    });
                let inner = frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(dates).strong());
                    ui.label(times);
                    ui.weak(best);
                });
                let r = inner
                    .response
                    .interact(egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Show the sun's path that day");
                if r.clicked() {
                    st.selected = Some(i);
                }
                ui.add_space(4.0);
            }
        });
}

/// The clues and every candidate as text.
fn report(s: &Saved, clues: &Clues, windows: &[Window]) -> String {
    let mut out = format!(
        "Chronolocation from shadows\nPlace: {:.5}, {:.5}\n",
        clues.lat, clues.lon
    );
    if let Some(az) = clues.azimuth {
        out += &format!(
            "Camera faces {:.1}°, shadows point {:.1}° ({}): sun at {az:.1}° ± {:.1}°\n",
            s.heading,
            shadow_bearing(s),
            sun::compass(shadow_bearing(s)),
            clues.azimuth_tolerance
        );
    }
    if let Some(el) = clues.elevation {
        out += &format!(
            "Object {} tall, shadow {} long: sun {el:.1}° ± {:.1}° high\n",
            s.height, s.shadow, clues.elevation_tolerance
        );
    }
    let any_year = s.when == When::Unknown;
    if any_year {
        out += "Searched any date (dates good to a day or so)\nCandidates:\n";
    } else {
        out += &format!(
            "Searched {}\nCandidates:\n",
            date_range(clues.days.0, clues.days.1, false)
        );
    }
    for w in windows {
        let [dates, times, best] = describe(w, any_year);
        out += &format!("- {dates}: {times}. {best}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_are_read_from_coordinates_and_links() {
        assert_eq!(parse_place("43.3026, 5.3806"), Some((43.3026, 5.3806)));
        assert_eq!(parse_place("43.3026 N 5.3806 W"), Some((43.3026, -5.3806)));
        assert_eq!(parse_place("-33.86,151.21"), Some((-33.86, 151.21)));
        assert_eq!(
            parse_place("https://www.google.com/maps/@43.3026,5.3806,17z"),
            Some((43.3026, 5.3806))
        );
        assert_eq!(parse_place("Marseille"), None);
        assert_eq!(parse_place("95, 10"), None);
    }

    #[test]
    fn the_map_gives_the_camera_bearing_with_north_up() {
        let line = |dx: f32, dy: f32| Some([[100.0, 100.0], [100.0 + dx, 100.0 + dy]]);
        let mut st = State::default();
        assert_eq!(map_heading(&st), None);
        for ((dx, dy), bearing) in [
            ((0.0, -50.0), 0.0),
            ((50.0, 0.0), 90.0),
            ((0.0, 50.0), 180.0),
            ((-30.0, -30.0), 315.0),
        ] {
            st.camera = line(dx, dy);
            assert!(
                (map_heading(&st).unwrap() - bearing).abs() < 1e-6,
                "{dx},{dy}"
            );
        }
    }

    #[test]
    fn unknown_dates_come_without_a_year() {
        let d = sun::days_from_civil(2026, 4, 18);
        assert_eq!(date(d, false), "18 Apr 2026");
        assert_eq!(date(d, true), "18 Apr");
        let jan = sun::days_from_civil(2027, 1, 3);
        assert_eq!(date_range(d - 20, d, true), "29 Mar – 18 Apr");
        assert_eq!(
            date_range(sun::days_from_civil(2026, 12, 10), jan, true),
            "10 Dec – 3 Jan"
        );
    }

    #[test]
    fn shadows_turn_with_the_camera() {
        let s = Saved {
            heading: 350.0,
            shadow_turn: 30.0,
            ..Saved::default()
        };
        assert!((shadow_bearing(&s) - 20.0).abs() < 1e-9);
        assert_eq!(turn_words(0.0), "straight away");
        assert_eq!(turn_words(-150.0), "toward, left");
        assert_eq!(turn_words(90.0), "right");
    }
}
