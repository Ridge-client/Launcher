//! "Annotate" tab: intelligence images. Pictures get numbered, coloured open squares drawn on
//! them, a caption for each, and are exported as PNGs with a legend. Several pictures can be open
//! at once (listed in the left panel); nothing is kept between sessions (the export is the result).

use std::path::{Path, PathBuf};

use anyhow::Context;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::icons;
use super::zoom::Zoom;
use super::RidgeApp;
use crate::annotate::{self, Annotated, Mark, COLOURS};

const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
/// Pictures are shown at most this big (the marks keep full-size coordinates).
const MAX_TEXTURE: u32 = 4096;
/// How close to a square's edge (on screen) a click picks it rather than drawing a new one.
const GRAB: f32 = 7.0;
/// Size of the thumbnails in the left panel.
const THUMB: u32 = 96;

#[derive(Default)]
pub(super) struct State {
    docs: Vec<Doc>,
    active: usize,
    tool: Tool,
    /// Colour of the next square.
    colour: usize,
    /// What the last action did (shown under the toolbar); true when it went wrong.
    status: Option<(String, bool)>,
    /// A picture with boxes to close, once the user agrees.
    closing: Option<usize>,
    /// Where the canvas was on the last frame, for the zoom buttons.
    canvas: Option<Rect>,
}

/// One open picture.
struct Doc {
    image: Annotated,
    selected: Option<usize>,
    drag: Option<Drag>,
    zoom: Zoom,
    texture: egui::TextureHandle,
    thumb: egui::TextureHandle,
    /// Full size of the picture in pixels.
    size: [f32; 2],
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Tool {
    /// Moves and zooms the picture, and moves, resizes and selects boxes.
    #[default]
    Pointer,
    /// Draws new boxes.
    Annotate,
}

#[derive(Clone, Copy)]
enum Drag {
    /// A new square from this corner (picture pixels).
    Draw([f32; 2]),
    Move(usize),
    Resize(usize),
    /// The picture itself.
    Pan,
}

impl Doc {
    fn load(file: &Path, ctx: &egui::Context) -> anyhow::Result<Self> {
        let picture = image::open(file).with_context(|| format!("opening {}", file.display()))?;
        let size = [picture.width() as f32, picture.height() as f32];
        let texture_of = |img: image::DynamicImage, name: String| {
            let rgba = img.to_rgba8();
            let pixels = [rgba.width() as usize, rgba.height() as usize];
            let colour = egui::ColorImage::from_rgba_unmultiplied(pixels, rgba.as_raw());
            ctx.load_texture(name, colour, egui::TextureOptions::LINEAR)
        };
        let path = file.display().to_string();
        let thumb = texture_of(picture.thumbnail(THUMB, THUMB), format!("{path}#thumb"));
        let shown = if picture.width().max(picture.height()) > MAX_TEXTURE {
            picture.thumbnail(MAX_TEXTURE, MAX_TEXTURE)
        } else {
            picture
        };
        let texture = texture_of(shown, path.clone());
        let name = file
            .file_stem()
            .map_or("Picture".into(), |s| s.to_string_lossy().into_owned());
        Ok(Self {
            image: Annotated {
                name,
                image: path,
                marks: Vec::new(),
            },
            selected: None,
            drag: None,
            zoom: Zoom::default(),
            texture,
            thumb,
            size,
        })
    }
}

/// Ridge's copies of pictures from when this tab kept a list of them, deleted once: the tab no
/// longer keeps anything (the originals are untouched).
pub(super) fn forget_old_copies() {
    if let Some(dir) = eframe::storage_dir("Ridge").map(|d| d.join("annotated")) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn colour(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Black on light colours, white on dark ones.
fn ink(rgb: [u8; 3]) -> Color32 {
    let luma = 0.299 * rgb[0] as f32 + 0.587 * rgb[1] as f32 + 0.114 * rgb[2] as f32;
    if luma > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn is_picture(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| PICTURES.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// Opens each of `files` next to the ones already open (one already open is just shown).
fn open_pictures(st: &mut State, ctx: &egui::Context, files: Vec<PathBuf>) {
    for file in files {
        if !is_picture(&file) {
            st.status = Some((
                format!(
                    "{} is not a picture Ridge can open (PNG, JPG, WebP, GIF)",
                    file.display()
                ),
                true,
            ));
            continue;
        }
        let path = file.display().to_string();
        if let Some(i) = st.docs.iter().position(|d| d.image.image == path) {
            st.active = i;
            continue;
        }
        match Doc::load(&file, ctx) {
            Ok(doc) => {
                st.docs.push(doc);
                st.active = st.docs.len() - 1;
                st.status = None;
            }
            Err(e) => st.status = Some((format!("Couldn't open the picture: {e:#}"), true)),
        }
    }
}

fn choose_pictures(st: &mut State, ctx: &egui::Context) {
    if let Some(files) = rfd::FileDialog::new()
        .set_title("Choose pictures to annotate")
        .add_filter("Pictures", &PICTURES)
        .pick_files()
    {
        open_pictures(st, ctx, files);
    }
}

fn close(st: &mut State, i: usize) {
    if i < st.docs.len() {
        st.docs.remove(i);
        if st.active > i || st.active >= st.docs.len() {
            st.active = st.active.saturating_sub(1);
        }
    }
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.annotate;
    // pictures dropped on the window are opened
    let dropped: Vec<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    if !dropped.is_empty() {
        open_pictures(st, ui.ctx(), dropped);
    }

    st.active = st.active.min(st.docs.len().saturating_sub(1));
    let side = |ui: &egui::Ui| {
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(12, 0))
    };
    egui::Panel::left("annotate_images")
        .resizable(false)
        .exact_size(160.0)
        .frame(side(ui))
        .show(ui, |ui| images_panel(st, ui));
    egui::Panel::right("annotate_marks")
        .resizable(false)
        .exact_size(360.0)
        .frame(side(ui))
        .show(ui, |ui| marks_panel(st, ui));
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 12,
        })
        .show(ui, |ui| {
            toolbar(st, ui);
            ui.add_space(6.0);
            canvas(st, ui);
        });
    close_dialog(st, ui);
}

/// The open pictures, as thumbnails: click one to work on it.
fn images_panel(st: &mut State, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    if ui
        .button(icons::with(icons::FOLDER_OPEN, "Import…"))
        .on_hover_text("Open more pictures (or drop them on the window)")
        .clicked()
    {
        choose_pictures(st, ui.ctx());
    }
    ui.add_space(4.0);
    let mut closing = None;
    egui::ScrollArea::vertical()
        .id_salt("annotate_images")
        .auto_shrink(false)
        .show(ui, |ui| {
            for (i, doc) in st.docs.iter().enumerate() {
                let active = i == st.active;
                let frame = egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(4))
                    .stroke(if active {
                        Stroke::new(1.5, Color32::from_rgb(0xe8, 0xa3, 0x5a))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    });
                let shown = frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let size = doc.thumb.size_vec2();
                    let fit = (ui.available_width() / size.x).min(72.0 / size.y);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Image::new((doc.thumb.id(), size * fit)));
                    });
                    ui.horizontal(|ui| {
                        let n = doc.image.marks.len();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button(icons::CANCEL)
                                .on_hover_text("Close this picture")
                                .clicked()
                            {
                                closing = Some(i);
                            }
                            if n > 0 {
                                ui.weak(format!("{n}"));
                            }
                            ui.add(
                                egui::Label::new(egui::RichText::new(&doc.image.name).small())
                                    .truncate(),
                            );
                        });
                    });
                });
                let r = shown.response.interact(egui::Sense::click());
                if r.on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(&doc.image.name)
                    .clicked()
                {
                    st.active = i;
                }
                ui.add_space(2.0);
            }
        });
    if let Some(i) = closing {
        if st.docs[i].image.marks.is_empty() {
            close(st, i);
        } else {
            st.closing = Some(i);
        }
    }
}

fn toolbar(st: &mut State, ui: &mut egui::Ui) {
    let typing = ui.memory(|m| m.focused().is_some());
    if !typing {
        if ui.input(|i| i.key_pressed(egui::Key::V)) {
            st.tool = Tool::Pointer;
        }
        if ui.input(|i| i.key_pressed(egui::Key::A)) {
            st.tool = Tool::Annotate;
        }
    }
    ui.horizontal(|ui| {
        if ui
            .selectable_label(st.tool == Tool::Pointer, "Pointer")
            .on_hover_text("V · Move and zoom the picture, select, move and resize boxes")
            .clicked()
        {
            st.tool = Tool::Pointer;
        }
        if ui
            .selectable_label(st.tool == Tool::Annotate, "Annotate")
            .on_hover_text("A · Drag on the picture to draw a box")
            .clicked()
        {
            st.tool = Tool::Annotate;
        }
        ui.separator();
        if let Some(doc) = st.docs.get_mut(st.active) {
            ui.add(egui::TextEdit::singleline(&mut doc.image.name).desired_width(160.0))
                .on_hover_text("Name of this image");
            ui.separator();
            if let Some(canvas) = st.canvas {
                doc.zoom.buttons(ui, canvas, doc.size, false);
                ui.separator();
            }
        }
        if ui
            .add_enabled(
                !st.docs.is_empty(),
                egui::Button::new(icons::with(icons::DOWNLOAD, "Export PNG…")),
            )
            .on_hover_text("The picture with its boxes and numbers, and a legend of the captions")
            .clicked()
        {
            export(st);
        }
        if st.docs.len() > 1
            && ui
                .button(icons::with(icons::DOWNLOAD, "Export all…"))
                .on_hover_text("Every open picture, as a PNG each, into a folder")
                .clicked()
        {
            export_all(st);
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let mut doc = st.docs.get_mut(st.active);
        ui.label("Colour");
        for (i, rgb) in COLOURS.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
            ui.painter()
                .rect_filled(rect.shrink(3.0), 4.0, colour(*rgb));
            if st.colour == i {
                ui.painter().rect_stroke(
                    rect,
                    5.0,
                    Stroke::new(2.0, ui.visuals().text_color()),
                    egui::StrokeKind::Inside,
                );
            }
            let r = r
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Colour of the next box (and of the selected one)");
            if r.clicked() {
                st.colour = i;
                if let Some(doc) = doc.as_deref_mut() {
                    if let Some(m) = doc.selected.and_then(|s| doc.image.marks.get_mut(s)) {
                        m.colour = *rgb;
                    }
                }
            }
        }
    });
    match &st.status {
        Some((text, true)) => {
            ui.colored_label(Color32::from_rgb(218, 54, 51), text);
        }
        Some((text, false)) => {
            ui.weak(text);
        }
        None => {}
    }
}

fn render_to(image: &Annotated, dest: &Path) -> anyhow::Result<()> {
    let picture = image::open(&image.image)
        .with_context(|| format!("opening {}", image.image))?
        .to_rgba8();
    let out = annotate::render(&picture, &image.marks)?;
    out.save_with_format(dest, image::ImageFormat::Png)
        .with_context(|| format!("saving {}", dest.display()))
}

fn export(st: &mut State) {
    let image = &st.docs[st.active].image;
    let Some(dest) = rfd::FileDialog::new()
        .set_title("Export the annotated image")
        .add_filter("PNG", &["png"])
        .set_file_name(format!("{}.png", image.name))
        .save_file()
    else {
        return;
    };
    st.status = Some(match render_to(image, &dest) {
        Ok(()) => (format!("Exported to {}", dest.display()), false),
        Err(e) => (format!("Couldn't export: {e:#}"), true),
    });
}

fn export_all(st: &mut State) {
    let Some(dir) = rfd::FileDialog::new()
        .set_title("Export every annotated image into a folder")
        .pick_folder()
    else {
        return;
    };
    let mut taken: Vec<PathBuf> = Vec::new();
    let mut failed = None;
    for doc in &st.docs {
        // two pictures with the same name get -2, -3… rather than overwriting each other
        let mut dest = dir.join(format!("{}.png", doc.image.name));
        let mut n = 2;
        while taken.contains(&dest) {
            dest = dir.join(format!("{}-{n}.png", doc.image.name));
            n += 1;
        }
        if let Err(e) = render_to(&doc.image, &dest) {
            failed = Some(format!("{}: {e:#}", doc.image.name));
            break;
        }
        taken.push(dest);
    }
    st.status = Some(match failed {
        None => (
            format!("Exported {} images to {}", taken.len(), dir.display()),
            false,
        ),
        Some(e) => (format!("Couldn't export: {e}"), true),
    });
}

fn canvas(st: &mut State, ui: &mut egui::Ui) {
    let area = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, ui.visuals().extreme_bg_color);
    let (tool, active_colour) = (st.tool, st.colour);
    let Some(doc) = st.docs.get_mut(st.active) else {
        st.canvas = None;
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            "Open pictures (or drop them on the window), then drag on one to box what matters.",
            egui::FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
        return;
    };
    let (tex, size) = (doc.texture.id(), doc.size);

    // the picture fitted in the area, centred, then zoomed
    st.canvas = Some(area);
    doc.zoom.interact(ui, &response, area, size);
    let (shown, scale) = doc.zoom.place(area, size);
    painter.image(
        tex,
        shown,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    let to_screen = |r: [f32; 4]| {
        Rect::from_min_max(
            shown.min + Vec2::new(r[0], r[1]) * scale,
            shown.min + Vec2::new(r[2], r[3]) * scale,
        )
    };
    let to_picture = |p: Pos2| {
        let q = (p - shown.min) / scale;
        [q.x.clamp(0.0, size[0]), q.y.clamp(0.0, size[1])]
    };

    let marks = &doc.image.marks;
    let badge_h = 20.0;
    let badge_of = |label: u32, r: Rect| {
        let w = 8.0 * label.to_string().len() as f32 + 10.0;
        let top = if r.top() - badge_h >= shown.top() {
            r.top() - badge_h
        } else {
            r.top()
        };
        Rect::from_min_size(Pos2::new(r.left(), top), Vec2::new(w.max(20.0), badge_h))
    };
    let handle_of = |r: Rect| Rect::from_center_size(r.max, Vec2::splat(12.0));

    // what is under the pointer (Pointer tool): the selected box's corner, a box's edge or number
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let selected = doc.selected;
    let hit = |p: Pos2| -> Option<Drag> {
        if let Some(s) = selected.filter(|s| *s < marks.len()) {
            if handle_of(to_screen(marks[s].rect)).contains(p) {
                return Some(Drag::Resize(s));
            }
        }
        marks.iter().enumerate().rev().find_map(|(i, m)| {
            let r = to_screen(m.rect);
            let on_edge = r.expand(GRAB).contains(p) && !r.shrink(GRAB).contains(p);
            (on_edge || badge_of(m.label(i), r).contains(p)).then_some(Drag::Move(i))
        })
    };
    let under = match tool {
        Tool::Pointer => pointer.and_then(hit),
        Tool::Annotate => None,
    };
    if response.hovered() && pointer.is_some_and(|p| shown.contains(p)) && doc.drag.is_none() {
        ui.ctx().set_cursor_icon(match (tool, under) {
            (_, Some(Drag::Resize(_))) => egui::CursorIcon::ResizeNwSe,
            (_, Some(_)) => egui::CursorIcon::Move,
            (Tool::Pointer, None) => egui::CursorIcon::Grab,
            (Tool::Annotate, None) => egui::CursorIcon::Crosshair,
        });
    }

    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(p) = pointer {
            doc.drag = match (tool, under) {
                (_, Some(Drag::Move(i))) => {
                    doc.selected = Some(i);
                    Some(Drag::Move(i))
                }
                (_, Some(d)) => Some(d),
                (Tool::Annotate, None) if shown.contains(p) => Some(Drag::Draw(to_picture(p))),
                (Tool::Pointer, None) => Some(Drag::Pan),
                _ => None,
            };
        }
    }
    if response.clicked() && tool == Tool::Pointer {
        doc.selected = match under {
            Some(Drag::Move(i) | Drag::Resize(i)) => Some(i),
            _ => None,
        };
    }
    let delta = response.drag_delta() / scale;
    let marks = &mut doc.image.marks;
    match &doc.drag {
        Some(Drag::Move(i)) => {
            if let Some(m) = marks.get_mut(*i) {
                let (w, h) = (m.rect[2] - m.rect[0], m.rect[3] - m.rect[1]);
                let l = (m.rect[0] + delta.x).clamp(0.0, size[0] - w);
                let t = (m.rect[1] + delta.y).clamp(0.0, size[1] - h);
                m.rect = [l, t, l + w, t + h];
            }
        }
        Some(Drag::Resize(i)) => {
            if let (Some(m), Some(p)) = (marks.get_mut(*i), pointer) {
                let q = to_picture(p);
                m.rect[2] = q[0].max(m.rect[0] + 4.0);
                m.rect[3] = q[1].max(m.rect[1] + 4.0);
            }
        }
        Some(Drag::Pan) => doc.zoom.pan_by(response.drag_delta(), area, size),
        _ => {}
    }

    // the box being drawn
    let drawing = match (&doc.drag, pointer) {
        (Some(Drag::Draw(start)), Some(p)) => {
            let end = to_picture(p);
            Some(Mark::normalised(
                [start[0], start[1], end[0], end[1]],
                size[0],
                size[1],
            ))
        }
        _ => None,
    };
    if response.drag_stopped() {
        if let Some(rect) = drawing {
            // a click-sized box is a slip, not a box
            if (rect[2] - rect[0]) * scale > 6.0 && (rect[3] - rect[1]) * scale > 6.0 {
                marks.push(Mark {
                    rect,
                    colour: COLOURS[active_colour],
                    ..Mark::default()
                });
                doc.selected = Some(marks.len() - 1);
            }
        }
        doc.drag = None;
    }

    // the boxes, numbered
    for (i, m) in marks.iter().enumerate() {
        let r = to_screen(m.rect);
        let c = colour(m.colour);
        painter.rect_stroke(r, 0.0, Stroke::new(2.5, c), egui::StrokeKind::Inside);
        let label = m.label(i);
        let badge = badge_of(label, r);
        painter.rect_filled(badge, 0.0, c);
        painter.text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            label.to_string(),
            egui::FontId::monospace(14.0),
            ink(m.colour),
        );
        if doc.selected == Some(i) {
            painter.rect_stroke(
                r.expand(3.0),
                0.0,
                Stroke::new(1.0, Color32::WHITE),
                egui::StrokeKind::Outside,
            );
            let h = handle_of(r);
            painter.rect_filled(h, 2.0, Color32::WHITE);
            painter.rect_stroke(h, 2.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
        }
    }
    if let Some(rect) = drawing {
        painter.rect_stroke(
            to_screen(rect),
            0.0,
            Stroke::new(2.0, colour(COLOURS[active_colour])),
            egui::StrokeKind::Inside,
        );
    }

    // Delete removes the selected box, when no caption is being typed
    let delete =
        ui.memory(|m| m.focused().is_none()) && ui.input(|i| i.key_pressed(egui::Key::Delete));
    if delete {
        if let Some(i) = doc.selected.take().filter(|i| *i < marks.len()) {
            marks.remove(i);
        }
    }
}

/// Every box: its number and colour, caption, recolour and remove.
fn marks_panel(st: &mut State, ui: &mut egui::Ui) {
    let Some(doc) = st.docs.get_mut(st.active) else {
        ui.add_space(8.0);
        ui.heading("Boxes");
        ui.weak("Open a picture, pick Annotate (A) and drag on it to box something.");
        return;
    };
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.heading("Boxes");
        if doc.image.marks.iter().any(|m| m.number.is_some())
            && ui
                .small_button("Renumber")
                .on_hover_text("Number the boxes 1, 2, 3… in list order again")
                .clicked()
        {
            for m in &mut doc.image.marks {
                m.number = None;
            }
        }
    });
    let marks_len = doc.image.marks.len();
    if marks_len == 0 {
        ui.weak("Pick Annotate (A) and drag on the picture to box something. Each box gets the next number; change it here.");
        return;
    }
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("annotate_marks")
        .auto_shrink(false)
        .show(ui, |ui| {
            for i in 0..marks_len {
                let selected = doc.selected == Some(i);
                let frame = egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .stroke(if selected {
                        Stroke::new(1.5, Color32::from_rgb(0xe8, 0xa3, 0x5a))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    });
                frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let m = &mut doc.image.marks[i];
                    ui.horizontal(|ui| {
                        // the number, editable: a box can carry any number, not just its place
                        let mut n = m.label(i);
                        let edit = ui
                            .add(egui::DragValue::new(&mut n).range(1..=999))
                            .on_hover_text("Number of this box (drag or type)");
                        if edit.changed() {
                            m.number = Some(n);
                        }
                        if edit.gained_focus() || edit.clicked() {
                            doc.selected = Some(i);
                        }
                        if m.number.is_some()
                            && ui
                                .small_button("auto")
                                .on_hover_text("Back to this box's place in the list")
                                .clicked()
                        {
                            m.number = None;
                        }
                        let (rect, r) =
                            ui.allocate_exact_size(Vec2::new(22.0, 22.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 3.0, colour(m.colour));
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            m.label(i).to_string(),
                            egui::FontId::monospace(12.0),
                            ink(m.colour),
                        );
                        if r.on_hover_text("Select this box").clicked() {
                            doc.selected = Some(i);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(icons::DELETE)
                                .on_hover_text("Remove this box")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                    // recolour
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for rgb in COLOURS {
                            let (sw, sr) =
                                ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                            ui.painter().rect_filled(sw.shrink(1.0), 2.0, colour(rgb));
                            if m.colour == rgb {
                                ui.painter().rect_stroke(
                                    sw,
                                    2.0,
                                    Stroke::new(1.5, ui.visuals().text_color()),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if sr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                m.colour = rgb;
                            }
                        }
                    });
                    let caption = ui.add(
                        egui::TextEdit::multiline(&mut m.caption)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .hint_text("What it shows: vehicle, entrance, person…"),
                    );
                    if caption.gained_focus() {
                        doc.selected = Some(i);
                    }
                });
                ui.add_space(4.0);
            }
        });
    if let Some(i) = remove {
        doc.image.marks.remove(i);
        doc.selected = None;
    }
}

/// "Close without exporting?" before a picture that has boxes is closed.
fn close_dialog(st: &mut State, ui: &mut egui::Ui) {
    let Some(i) = st.closing.filter(|i| *i < st.docs.len()) else {
        st.closing = None;
        return;
    };
    let (mut close_it, mut cancel) = (false, false);
    egui::Modal::new(egui::Id::unique("annotate_close")).show(ui.ctx(), |ui| {
        ui.set_width(400.0);
        ui.heading("Close this picture?");
        ui.label(format!(
            "The {} boxes on {} are lost unless you export it first.",
            st.docs[i].image.marks.len(),
            st.docs[i].image.name
        ));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(icons::with(icons::DOWNLOAD, "Export first"))
                .clicked()
            {
                st.active = i;
                export(st);
            }
            close_it = ui.button("Close without exporting").clicked();
            cancel = ui.button(icons::with(icons::CANCEL, "Cancel")).clicked();
        });
    });
    if close_it {
        st.closing = None;
        close(st, i);
    } else if cancel {
        st.closing = None;
    }
}
