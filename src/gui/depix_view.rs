//! "Depix" tab: recovering text from a pixelated screenshot. The pixelated picture is opened and
//! the pixelated text selected; a reference picture of the same font (a De Bruijn text typed in
//! the same editor, screenshotted at the same size) gives the windows its blocks are matched to.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use anyhow::Context;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use image::RgbaImage;

use super::icons;
use super::zoom::Zoom;
use super::RidgeApp;
use crate::depix::{self, Grid};

const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];
/// Pictures are shown at most this big (the selection keeps full-size coordinates).
const MAX_TEXTURE: u32 = 4096;
const DEFAULT_ALPHABET: &str = "0123456789abcdef";

struct Pic {
    name: String,
    image: Arc<RgbaImage>,
    texture: egui::TextureHandle,
}

impl Pic {
    fn load(path: &Path, ctx: &egui::Context) -> anyhow::Result<Self> {
        let picture = image::open(path).with_context(|| format!("opening {}", path.display()))?;
        let rgba = Arc::new(picture.to_rgba8());
        let texture = texture_of(&rgba, path.display().to_string(), ctx);
        let name = path
            .file_name()
            .map_or("Picture".into(), |n| n.to_string_lossy().into_owned());
        Ok(Self {
            name,
            image: rgba,
            texture,
        })
    }

    fn size(&self) -> [f32; 2] {
        [self.image.width() as f32, self.image.height() as f32]
    }
}

fn texture_of(img: &RgbaImage, name: String, ctx: &egui::Context) -> egui::TextureHandle {
    let shown = if img.width().max(img.height()) > MAX_TEXTURE {
        image::DynamicImage::ImageRgba8(img.clone())
            .thumbnail(MAX_TEXTURE, MAX_TEXTURE)
            .to_rgba8()
    } else {
        img.clone()
    };
    let pixels = [shown.width() as usize, shown.height() as usize];
    let colour = egui::ColorImage::from_rgba_unmultiplied(pixels, shown.as_raw());
    ctx.load_texture(name, colour, egui::TextureOptions::NEAREST)
}

pub(super) struct State {
    pixelated: Option<Pic>,
    reference: Option<Pic>,
    /// The pixelated text, in the pixelated picture's pixels: left, top, right, bottom.
    selection: Option<[f32; 4]>,
    drag_from: Option<[f32; 2]>,
    /// The grid found in the selection.
    grid: Option<Grid>,
    /// A block size typed by hand; 0 leaves it to the detection.
    block: u32,
    /// How many of the best matching windows are averaged for each block.
    top: usize,
    result: Option<Pic>,
    job: Option<Receiver<Result<RgbaImage, String>>>,
    zooms: [Zoom; 2],
    canvases: [Option<Rect>; 2],
    alphabet: String,
    order: usize,
    status: Option<(String, bool)>,
    /// The reference-text window is open.
    help: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            pixelated: None,
            reference: None,
            selection: None,
            drag_from: None,
            grid: None,
            block: 0,
            top: 1,
            result: None,
            job: None,
            zooms: [Zoom::default(); 2],
            canvases: [None; 2],
            alphabet: DEFAULT_ALPHABET.into(),
            order: 2,
            status: None,
            help: false,
        }
    }
}

fn choose(title: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("Pictures", &PICTURES)
        .pick_file()
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.depix;
    poll(st, ui.ctx());
    ui.add_space(4.0);
    toolbar(st, ui);
    ui.add_space(2.0);
    // one line, always: the status never changes the height of what is below
    let (text, colour) = match &st.status {
        Some((text, true)) => (text.as_str(), Color32::from_rgb(218, 54, 51)),
        Some((text, false)) => (text.as_str(), ui.visuals().weak_text_color()),
        None => (
            "Open the pixelated picture and the reference, select the text, then Recover",
            ui.visuals().weak_text_color(),
        ),
    };
    ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).truncate())
        .on_hover_text(text);
    reference_help(st, ui.ctx());
    ui.add_space(4.0);
    let st = &mut *st;
    ui.columns(2, |cols| {
        pane(st, 0, &mut cols[0]);
        pane(st, 1, &mut cols[1]);
    });
}

/// Takes in the finished recovery, if there is one.
fn poll(st: &mut State, ctx: &egui::Context) {
    let Some(rx) = &st.job else {
        return;
    };
    match rx.try_recv() {
        Ok(Ok(img)) => {
            let texture = texture_of(&img, "depix-result".into(), ctx);
            st.result = Some(Pic {
                name: "Recovered".into(),
                image: Arc::new(img),
                texture,
            });
            st.zooms[1] = Zoom::default();
            st.status = Some(("Done: the recovered blocks are on the right".into(), false));
            st.job = None;
        }
        Ok(Err(e)) => {
            st.status = Some((format!("Couldn't recover: {e}"), true));
            st.job = None;
        }
        Err(mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(150))
        }
        Err(mpsc::TryRecvError::Disconnected) => st.job = None,
    }
}

fn toolbar(st: &mut State, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui
            .button(icons::with(icons::FOLDER_OPEN, "Pixelated picture…"))
            .on_hover_text(match &st.pixelated {
                Some(p) => format!("The screenshot with the pixelated text (now {})", p.name),
                None => "The screenshot with the pixelated text".into(),
            })
            .clicked()
        {
            if let Some(path) = choose("Choose the pixelated picture") {
                match Pic::load(&path, ui.ctx()) {
                    Ok(pic) => {
                        st.pixelated = Some(pic);
                        st.selection = None;
                        st.grid = None;
                        st.result = None;
                        st.zooms[0] = Zoom::default();
                        st.status = None;
                    }
                    Err(e) => st.status = Some((format!("{e:#}"), true)),
                }
            }
        }
        if ui
            .button(icons::with(icons::FOLDER_OPEN, "Reference…"))
            .on_hover_text(match &st.reference {
                Some(p) => format!(
                    "A screenshot of the De Bruijn text in the same font and size (now {})",
                    p.name
                ),
                None => "A screenshot of the De Bruijn text in the same font and size".into(),
            })
            .clicked()
        {
            if let Some(path) = choose("Choose the reference picture") {
                match Pic::load(&path, ui.ctx()) {
                    Ok(pic) => {
                        st.reference = Some(pic);
                        st.status = None;
                    }
                    Err(e) => st.status = Some((format!("{e:#}"), true)),
                }
            }
        }
        if ui
            .selectable_label(st.help, "Reference text…")
            .on_hover_text("The text to type in the target's editor to make the reference")
            .clicked()
        {
            st.help = !st.help;
        }
        if ui
            .add_enabled(
                st.result.is_some(),
                egui::Button::new(icons::with(icons::DOWNLOAD, "Export PNG…")),
            )
            .clicked()
        {
            export(st);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Block");
        ui.add(egui::DragValue::new(&mut st.block).range(0..=64).custom_formatter(|v, _| {
            if v == 0.0 {
                "auto".into()
            } else {
                format!("{v:.0}")
            }
        }))
        .on_hover_text("Side of a pixelation block, in pixels (auto reads it from the selection)");
        if let Some(g) = st.grid {
            ui.weak(format!("found {} px", g.block));
        }
        ui.label("Matches");
        ui.add(egui::DragValue::new(&mut st.top).range(1..=9))
            .on_hover_text("How many of the best matches are averaged for each block: 1 is sharpest, more is steadier");
        ui.separator();
        let ready = st.pixelated.is_some()
            && st.reference.is_some()
            && st.selection.is_some()
            && st.job.is_none();
        if ui
            .add_enabled(ready, egui::Button::new("Recover"))
            .on_hover_text("Match every block of the selection against the reference")
            .clicked()
        {
            start(st);
        }
        if st.job.is_some() {
            ui.spinner();
        }
    });
}

/// The text to type in the target's editor, in a window of its own.
fn reference_help(st: &mut State, ctx: &egui::Context) {
    let mut open = st.help;
    egui::Window::new("Reference text")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.weak("Type this text in the editor and font that produced the pixelated text, at the same size and colours, screenshot it, and open that as the reference. It holds every run of characters once, so every block has a match.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Characters");
                ui.add(egui::TextEdit::singleline(&mut st.alphabet).desired_width(220.0));
            });
            ui.horizontal(|ui| {
                ui.label("Run");
                ui.add(egui::DragValue::new(&mut st.order).range(2..=4));
                let chars: Vec<char> = dedup(&st.alphabet);
                let len = chars.len().saturating_pow(st.order as u32);
                ui.weak(format!("{len} characters"));
                if ui
                    .add_enabled(
                        (1..=200_000).contains(&len),
                        egui::Button::new(icons::with(icons::COPY, "Copy text")),
                    )
                    .on_hover_text("Lines of 100 characters, ready to paste in the editor")
                    .clicked()
                {
                    let text = depix::wrapped(&depix::de_bruijn(&chars, st.order), 100);
                    ctx.copy_text(text);
                    st.status = Some(("Reference text copied".into(), false));
                }
            });
        });
    st.help = open;
}

fn dedup(s: &str) -> Vec<char> {
    let mut seen = Vec::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen
}

fn start(st: &mut State) {
    let (Some(pic), Some(reference), Some(sel)) = (&st.pixelated, &st.reference, st.selection)
    else {
        return;
    };
    let region = [
        sel[0] as u32,
        sel[1] as u32,
        sel[2].ceil() as u32,
        sel[3].ceil() as u32,
    ];
    let Some(mut grid) = st.grid.or_else(|| depix::detect_grid(&pic.image, region)) else {
        st.status = Some((
            "Couldn't find the pixel blocks in the selection: set the block size by hand".into(),
            true,
        ));
        return;
    };
    if st.block >= 2 {
        // a size typed by hand: the grid starts at the selection's corner
        grid = Grid {
            block: st.block,
            x0: region[0],
            y0: region[1],
        };
    }
    let (target, reference, top) = (pic.image.clone(), reference.image.clone(), st.top);
    let (tx, rx) = mpsc::channel();
    st.job = Some(rx);
    st.status = Some(("Matching the blocks…".into(), false));
    std::thread::spawn(move || {
        let _ = tx.send(
            depix::recover(&target, region, grid, &reference, top).map_err(|e| format!("{e:#}")),
        );
    });
}

fn export(st: &mut State) {
    let Some(result) = &st.result else {
        return;
    };
    let Some(dest) = rfd::FileDialog::new()
        .set_title("Export the recovered text")
        .add_filter("PNG", &["png"])
        .set_file_name("recovered.png")
        .save_file()
    else {
        return;
    };
    st.status = Some(
        match result
            .image
            .save_with_format(&dest, image::ImageFormat::Png)
        {
            Ok(()) => (format!("Exported to {}", dest.display()), false),
            Err(e) => (format!("Couldn't export: {e}"), true),
        },
    );
}

/// One of the two pictures, with its title and zoom: 0 the pixelated picture (drag to select the
/// text), 1 the recovered blocks.
fn pane(st: &mut State, index: usize, ui: &mut egui::Ui) {
    let pic = if index == 0 {
        &st.pixelated
    } else {
        &st.result
    };
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.label(
            egui::RichText::new(if index == 0 { "Pixelated" } else { "Recovered" })
                .strong()
                .size(15.0),
        );
        if let (Some(pic), Some(canvas)) = (pic, st.canvases[index]) {
            let size = pic.size();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                st.zooms[index].buttons(ui, canvas, size, true);
            });
        }
    });
    let area = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 4.0, ui.visuals().extreme_bg_color);
    st.canvases[index] = Some(area);
    let Some(pic) = (if index == 0 {
        &st.pixelated
    } else {
        &st.result
    }) else {
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            if index == 0 {
                "Open the pixelated picture"
            } else {
                "The recovered text appears here"
            },
            egui::FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
        return;
    };
    let size = pic.size();
    let tex = pic.texture.id();
    st.zooms[index].interact(ui, &response, area, size);
    let (shown, scale) = st.zooms[index].place(area, size);
    painter.image(
        tex,
        shown,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    if index != 0 {
        return;
    }

    let to_screen = |p: [f32; 2]| shown.min + Vec2::new(p[0], p[1]) * scale;
    let to_picture = |p: Pos2| {
        let q = (p - shown.min) / scale;
        [q.x.clamp(0.0, size[0]), q.y.clamp(0.0, size[1])]
    };
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if response.drag_started_by(egui::PointerButton::Primary) {
        st.drag_from = pointer.map(to_picture);
    }
    if let (Some(from), Some(p)) = (st.drag_from, pointer) {
        let to = to_picture(p);
        st.selection = Some([
            from[0].min(to[0]),
            from[1].min(to[1]),
            from[0].max(to[0]),
            from[1].max(to[1]),
        ]);
    }
    if response.drag_stopped() && st.drag_from.take().is_some() {
        st.grid = None;
        if let (Some(sel), Some(pic)) = (st.selection, &st.pixelated) {
            if sel[2] - sel[0] < 4.0 || sel[3] - sel[1] < 4.0 {
                st.selection = None;
            } else {
                let region = [sel[0] as u32, sel[1] as u32, sel[2] as u32, sel[3] as u32];
                st.grid = depix::detect_grid(&pic.image, region);
                st.status = None;
            }
        }
    }
    if let Some(sel) = st.selection {
        let rect = Rect::from_two_pos(to_screen([sel[0], sel[1]]), to_screen([sel[2], sel[3]]));
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(2.0, Color32::from_rgb(0xe8, 0xa3, 0x5a)),
            egui::StrokeKind::Outside,
        );
        // the blocks found, when they are big enough on screen to tell apart
        if let Some(g) = st.grid.filter(|g| g.block as f32 * scale >= 5.0) {
            let line = Stroke::new(1.0, Color32::from_white_alpha(60));
            let step = g.block as f32;
            let mut x = sel[0] + (g.x0 as f32 - sel[0]).rem_euclid(step);
            while x < sel[2] {
                painter.vline(to_screen([x, 0.0]).x, rect.y_range(), line);
                x += step;
            }
            let mut y = sel[1] + (g.y0 as f32 - sel[1]).rem_euclid(step);
            while y < sel[3] {
                painter.hline(rect.x_range(), to_screen([0.0, y]).y, line);
                y += step;
            }
        }
    }
}
