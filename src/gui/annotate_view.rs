//! "Annotate" tab: an intelligence image. A picture gets numbered, coloured open squares drawn on
//! it, a caption for each, and is exported as one PNG with a legend. One picture at a time;
//! nothing is kept between sessions (the export is the result).

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

#[derive(Default)]
pub(super) struct State {
    /// The picture being annotated (its `image` path is empty until one is opened).
    image: Annotated,
    selected: Option<usize>,
    /// Colour of the next square.
    colour: usize,
    drag: Option<Drag>,
    /// The current picture on screen: its path, texture and full size.
    texture: Option<(String, egui::TextureHandle, [f32; 2])>,
    /// What the last action did (shown under the toolbar); true when it went wrong.
    status: Option<(String, bool)>,
    /// A picture to open in place of one that has boxes, once the user agrees.
    replacing: Option<PathBuf>,
    zoom: Zoom,
    /// Where the canvas was on the last frame, for the zoom buttons.
    canvas: Option<Rect>,
}

#[derive(Clone, Copy)]
enum Drag {
    /// A new square from this corner (picture pixels).
    Draw([f32; 2]),
    Move(usize),
    Resize(usize),
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

fn is_picture(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| PICTURES.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// Opens `file` for annotating; asks first when the current picture has boxes.
fn open_picture(st: &mut State, file: PathBuf, confirmed: bool) {
    if !is_picture(&file) {
        st.status = Some((
            format!(
                "{} is not a picture Ridge can open (PNG, JPG, WebP, GIF)",
                file.display()
            ),
            true,
        ));
        return;
    }
    if !st.image.marks.is_empty() && !confirmed {
        st.replacing = Some(file);
        return;
    }
    let name = file
        .file_stem()
        .map_or("Picture".into(), |s| s.to_string_lossy().into_owned());
    st.image = Annotated {
        name,
        image: file.display().to_string(),
        marks: Vec::new(),
    };
    st.selected = None;
    st.drag = None;
    st.status = None;
    st.zoom = Zoom::default();
}

fn choose_picture(st: &mut State) {
    if let Some(file) = rfd::FileDialog::new()
        .set_title("Choose a picture to annotate")
        .add_filter("Pictures", &PICTURES)
        .pick_file()
    {
        open_picture(st, file, false);
    }
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.annotate;
    // a picture dropped on the window is opened
    let dropped: Option<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .find(|p| !p.as_os_str().is_empty())
    });
    if let Some(file) = dropped {
        open_picture(st, file, false);
    }

    if st.image.image.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(icons::IMAGE).size(48.0).weak());
            ui.heading("Annotate a picture");
            ui.weak(
                "Open a picture (or drop one on the window), then drag on it to box what matters.",
            );
            ui.weak("Each box gets a number and a colour; export it all as one PNG with a legend.");
            ui.add_space(12.0);
            if ui
                .button(icons::with(icons::FOLDER_OPEN, "Open picture…"))
                .clicked()
            {
                choose_picture(st);
            }
            if let Some((text, true)) = &st.status {
                ui.colored_label(Color32::from_rgb(218, 54, 51), text);
            }
        });
        return;
    }
    egui::Panel::right("annotate_marks")
        .resizable(false)
        .exact_size(340.0)
        .show(ui, |ui| marks_panel(st, ui));
    ui.add_space(4.0);
    toolbar(st, ui);
    ui.add_space(4.0);
    canvas(st, ui);
    replace_dialog(st, ui);
}

fn toolbar(st: &mut State, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui
            .button(icons::FOLDER_OPEN)
            .on_hover_text("Open another picture (or drop one on the window)")
            .clicked()
        {
            choose_picture(st);
        }
        let name = &mut st.image.name;
        ui.add(egui::TextEdit::singleline(name).desired_width(200.0))
            .on_hover_text("Name of this image");
        ui.separator();
        ui.label("Colour");
        for (i, rgb) in COLOURS.iter().enumerate() {
            let (rect, r) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::click());
            let active = st.colour == i;
            ui.painter()
                .rect_filled(rect.shrink(3.0), 4.0, colour(*rgb));
            if active {
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
                if let Some(m) = st.selected.and_then(|s| st.image.marks.get_mut(s)) {
                    m.colour = *rgb;
                }
            }
        }
        ui.separator();
        if let (Some((.., size)), Some(canvas)) = (&st.texture, st.canvas) {
            let size = *size;
            st.zoom.buttons(ui, canvas, size, false);
            ui.separator();
        }
        if ui
            .button(icons::with(icons::DOWNLOAD, "Export PNG…"))
            .on_hover_text("The picture with its boxes and numbers, and a legend of the captions")
            .clicked()
        {
            export(st);
        }
    });
    match &st.status {
        Some((text, true)) => {
            ui.colored_label(Color32::from_rgb(218, 54, 51), text);
        }
        Some((text, false)) => {
            ui.weak(text);
        }
        None => {
            ui.weak("Drag on the picture to box something · drag a box's edge to move it, its corner to resize · Delete removes the selected box · Ctrl+scroll zooms");
        }
    }
}

fn export(st: &mut State) {
    let image = &st.image;
    let Some(dest) = rfd::FileDialog::new()
        .set_title("Export the annotated image")
        .add_filter("PNG", &["png"])
        .set_file_name(format!("{}.png", image.name))
        .save_file()
    else {
        return;
    };
    let result = (|| {
        let picture = image::open(&image.image)
            .with_context(|| format!("opening {}", image.image))?
            .to_rgba8();
        let out = annotate::render(&picture, &image.marks)?;
        out.save_with_format(&dest, image::ImageFormat::Png)
            .with_context(|| format!("saving {}", dest.display()))
    })();
    st.status = Some(match result {
        Ok(()) => (format!("Exported to {}", dest.display()), false),
        Err(e) => (format!("Couldn't export: {e:#}"), true),
    });
}

/// The picture's texture, loaded once (shrunk to fit a texture if it is huge).
fn texture(st: &mut State, ctx: &egui::Context) -> Option<(egui::TextureId, [f32; 2])> {
    let path = st.image.image.clone();
    if st.texture.as_ref().is_none_or(|(p, ..)| *p != path) {
        st.texture = None;
        let picture = match image::open(&path) {
            Ok(p) => p,
            Err(e) => {
                st.status = Some((format!("Couldn't open the picture: {e}"), true));
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
        let handle = ctx.load_texture(&path, colour, egui::TextureOptions::LINEAR);
        st.texture = Some((path, handle, size));
    }
    st.texture.as_ref().map(|(_, t, size)| (t.id(), *size))
}

fn canvas(st: &mut State, ui: &mut egui::Ui) {
    let area = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, ui.visuals().extreme_bg_color);
    let Some((tex, size)) = texture(st, ui.ctx()) else {
        return;
    };

    // the picture fitted in the area, centred, then zoomed
    st.canvas = Some(area);
    st.zoom.interact(ui, &response, area, size);
    let (shown, scale) = st.zoom.place(area, size);
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

    let marks = &st.image.marks;
    let badge_h = 20.0;
    let badge_of = |i: usize, r: Rect| {
        let w = if i + 1 >= 10 { 28.0 } else { 20.0 };
        let top = if r.top() - badge_h >= shown.top() {
            r.top() - badge_h
        } else {
            r.top()
        };
        Rect::from_min_size(Pos2::new(r.left(), top), Vec2::new(w, badge_h))
    };
    let handle_of = |r: Rect| Rect::from_center_size(r.max, Vec2::splat(12.0));

    // what is under the pointer: the selected box's corner, a box's edge or number, or nothing
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let selected = st.selected;
    let hit = |p: Pos2| -> Option<Drag> {
        if let Some(s) = selected.filter(|s| *s < marks.len()) {
            if handle_of(to_screen(marks[s].rect)).contains(p) {
                return Some(Drag::Resize(s));
            }
        }
        marks.iter().enumerate().rev().find_map(|(i, m)| {
            let r = to_screen(m.rect);
            let on_edge = r.expand(GRAB).contains(p) && !r.shrink(GRAB).contains(p);
            (on_edge || badge_of(i, r).contains(p)).then_some(Drag::Move(i))
        })
    };
    let under = pointer.and_then(hit);
    if pointer.is_some_and(|p| shown.contains(p)) && st.drag.is_none() {
        ui.ctx().set_cursor_icon(match under {
            Some(Drag::Resize(_)) => egui::CursorIcon::ResizeNwSe,
            Some(_) => egui::CursorIcon::Move,
            None => egui::CursorIcon::Crosshair,
        });
    }

    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(p) = pointer {
            st.drag = match under {
                Some(Drag::Move(i)) => {
                    st.selected = Some(i);
                    Some(Drag::Move(i))
                }
                Some(d) => Some(d),
                None if shown.contains(p) => Some(Drag::Draw(to_picture(p))),
                None => None,
            };
        }
    }
    if response.clicked() {
        st.selected = match under {
            Some(Drag::Move(i) | Drag::Resize(i)) => Some(i),
            _ => None,
        };
    }
    let delta = response.drag_delta() / scale;
    let marks = &mut st.image.marks;
    match &st.drag {
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
        _ => {}
    }

    // the box being drawn
    let drawing = match (&st.drag, pointer) {
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
                    colour: COLOURS[st.colour],
                    caption: String::new(),
                });
                st.selected = Some(marks.len() - 1);
            }
        }
        st.drag = None;
    }

    // the boxes, numbered
    for (i, m) in marks.iter().enumerate() {
        let r = to_screen(m.rect);
        let c = colour(m.colour);
        painter.rect_stroke(r, 0.0, Stroke::new(2.5, c), egui::StrokeKind::Inside);
        let badge = badge_of(i, r);
        painter.rect_filled(badge, 0.0, c);
        let luma =
            0.299 * m.colour[0] as f32 + 0.587 * m.colour[1] as f32 + 0.114 * m.colour[2] as f32;
        painter.text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            (i + 1).to_string(),
            egui::FontId::monospace(14.0),
            if luma > 150.0 {
                Color32::BLACK
            } else {
                Color32::WHITE
            },
        );
        if st.selected == Some(i) {
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
            Stroke::new(2.0, colour(COLOURS[st.colour])),
            egui::StrokeKind::Inside,
        );
    }

    // Delete removes the selected box, when no caption is being typed
    let delete =
        ui.memory(|m| m.focused().is_none()) && ui.input(|i| i.key_pressed(egui::Key::Delete));
    if delete {
        if let Some(i) = st.selected.take().filter(|i| *i < marks.len()) {
            marks.remove(i);
        }
    }
}

/// Every box: its number and colour, caption, recolour and remove.
fn marks_panel(st: &mut State, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.heading("Boxes");
    let marks_len = st.image.marks.len();
    if marks_len == 0 {
        ui.weak("Drag on the picture to box something. Each box gets the next number.");
        return;
    }
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("annotate_marks")
        .auto_shrink(false)
        .show(ui, |ui| {
            for i in 0..marks_len {
                let selected = st.selected == Some(i);
                let frame = egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .stroke(if selected {
                        Stroke::new(1.5, Color32::from_rgb(0xe8, 0xa3, 0x5a))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    });
                frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let m = &mut st.image.marks[i];
                    ui.horizontal(|ui| {
                        let (rect, r) =
                            ui.allocate_exact_size(Vec2::new(28.0, 22.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 3.0, colour(m.colour));
                        let luma = 0.299 * m.colour[0] as f32
                            + 0.587 * m.colour[1] as f32
                            + 0.114 * m.colour[2] as f32;
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            (i + 1).to_string(),
                            egui::FontId::monospace(14.0),
                            if luma > 150.0 {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            },
                        );
                        if r.on_hover_text("Select this box").clicked() {
                            st.selected = Some(i);
                        }
                        // recolour
                        for rgb in COLOURS {
                            let (sw, sr) =
                                ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::click());
                            ui.painter().rect_filled(sw.shrink(1.0), 2.0, colour(rgb));
                            if m.colour == rgb {
                                ui.painter().rect_stroke(
                                    sw,
                                    2.0,
                                    Stroke::new(1.5, ui.visuals().text_color()),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if sr.clicked() {
                                m.colour = rgb;
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button(icons::DELETE)
                                .on_hover_text("Remove this box")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                    let caption = ui.add(
                        egui::TextEdit::multiline(&mut m.caption)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .hint_text("What it shows: vehicle, entrance, person…"),
                    );
                    if caption.gained_focus() {
                        st.selected = Some(i);
                    }
                });
                ui.add_space(4.0);
            }
        });
    if let Some(i) = remove {
        st.image.marks.remove(i);
        st.selected = None;
    }
}

/// "Discard the boxes?" before another picture replaces one that has them.
fn replace_dialog(st: &mut State, ui: &mut egui::Ui) {
    let Some(file) = st.replacing.clone() else {
        return;
    };
    let (mut open, mut cancel) = (false, false);
    egui::Modal::new(egui::Id::unique("annotate_replace")).show(ui.ctx(), |ui| {
        ui.set_width(400.0);
        ui.heading("Open another picture?");
        ui.label(format!(
            "The {} boxes on this one are lost unless you export it first.",
            st.image.marks.len()
        ));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(icons::with(icons::DOWNLOAD, "Export first"))
                .clicked()
            {
                export(st);
            }
            open = ui.button("Open without exporting").clicked();
            cancel = ui.button(icons::with(icons::CANCEL, "Cancel")).clicked();
        });
    });
    if open {
        st.replacing = None;
        open_picture(st, file, true);
    } else if cancel {
        st.replacing = None;
    }
}
