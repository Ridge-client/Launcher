//! "Graph" tab: an investigation board. Entities from the palette on the left are placed, moved,
//! lined up and linked on the board; the panel on the right edits the selected one's facts,
//! picture, notes and links. Ridge opens on a blank graph; graphs are saved to and opened from
//! files.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::icons;
use super::RidgeApp;
use crate::graph::{self, Arrow, Graph, Kind, Layout, LineStyle, CATEGORIES, KINDS};

const SNAP_KEY: &str = "graph_snap";
const RECENT_KEY: &str = "graph_recent";
/// How many kinds "Recently used" lists.
const RECENT: usize = 8;
const GRID: f32 = 20.0;
const ACCENT: Color32 = Color32::from_rgb(0xe8, 0xa3, 0x5a);
/// Labels offered for a link (it takes any text).
const LINK_LABELS: &[&str] = &[
    "owns",
    "uses",
    "lives at",
    "works at",
    "knows",
    "same as",
    "registered to",
    "paid",
    "posted",
    "seen at",
];

#[derive(Default)]
pub(super) struct State {
    graph: Graph,
    /// The file it was opened from or last saved to.
    path: Option<PathBuf>,
    /// The graph as last saved (or opened), to tell unsaved changes.
    saved: Graph,
    dirty: bool,
    /// The user chose to close Ridge without saving.
    closing: bool,
    /// New or Open asked while there are unsaved changes: waits for the user's answer.
    pending: Option<Pending>,
    /// What the last file action did; true when it went wrong.
    status: Option<(String, bool)>,
    /// Each entity's picture as a texture, with the size of the data it was made from.
    pictures: HashMap<u64, (usize, Option<egui::TextureHandle>)>,
    selected: BTreeSet<u64>,
    /// The link selected on the board (index in the graph's links).
    selected_link: Option<usize>,
    tool: Tool,
    /// The organic layout keeps running while entities are moved.
    live_layout: bool,
    /// How much it still moves things (1: just changed, near 0: settled), and the entity and
    /// link counts it last saw, to wake it when they change.
    live_heat: f32,
    live_seen: (usize, usize),
    /// Kinds added lately, newest first.
    recent: Vec<String>,
    /// What the palette's search box holds.
    search: String,
    /// Board position shown at the canvas's top-left corner, and the zoom.
    pan: Vec2,
    zoom: f32,
    snap: bool,
    /// A link being drawn from this entity's handle.
    linking: Option<u64>,
    /// Where a Shift+drag selection box started (screen).
    boxing: Option<Pos2>,
    /// A palette entry being dragged onto the board.
    dragging_kind: Option<&'static str>,
    /// The board's size on screen, as last drawn.
    canvas_size: Vec2,
    /// Size of each entity as last drawn, in board units (for aligning).
    sizes: BTreeMap<u64, [f32; 2]>,
}

/// What dragging on the board does.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tool {
    /// Drag an entity to move it, the background to draw a selection box.
    #[default]
    Select,
    /// Drag anywhere to move the view.
    Pan,
    /// Drag from one entity onto another to link them.
    Link,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    New,
    Open,
    /// Closing Ridge.
    Close,
}

impl State {
    /// A blank graph (graphs live in files, not in Ridge's settings); only "Snap" is kept.
    pub(super) fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        Self {
            snap: storage
                .and_then(|s| eframe::get_value(s, SNAP_KEY))
                .unwrap_or(true),
            recent: storage
                .and_then(|s| eframe::get_value(s, RECENT_KEY))
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    pub(super) fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SNAP_KEY, &self.snap);
        eframe::set_value(storage, RECENT_KEY, &self.recent);
    }

    fn clear_selection(&mut self) {
        self.selected.clear();
        self.selected_link = None;
    }

    /// Shows `graph` (from `path`) as a fresh, saved board.
    fn replace(&mut self, graph: Graph, path: Option<PathBuf>) {
        self.saved = graph.clone();
        self.graph = graph;
        self.path = path;
        self.dirty = false;
        self.pictures.clear();
        self.clear_selection();
        self.zoom = 0.0; // fitted to the board on the next frame
    }

    /// Its name, or "Untitled graph".
    fn title(&self) -> String {
        let name = self.graph.name.trim();
        if name.is_empty() {
            "Untitled graph".into()
        } else {
            name.to_string()
        }
    }
}

/// Writes the graph to its file, or asks where first (`save_as`, or never saved).
fn save(st: &mut State, save_as: bool) -> bool {
    let path = match (&st.path, save_as) {
        (Some(path), false) => path.clone(),
        _ => {
            let Some(path) = rfd::FileDialog::new()
                .set_title("Save the graph")
                .add_filter("Ridge graph", &[graph::FILE_EXTENSION])
                .set_file_name(format!("{}.{}", st.title(), graph::FILE_EXTENSION))
                .save_file()
            else {
                return false;
            };
            path
        }
    };
    let result = st
        .graph
        .to_file()
        .map_err(|e| e.to_string())
        .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()));
    match result {
        Ok(()) => {
            st.saved = st.graph.clone();
            st.dirty = false;
            st.status = Some((format!("Saved to {}", path.display()), false));
            st.path = Some(path);
            true
        }
        Err(e) => {
            st.status = Some((format!("Couldn't save to {}: {e}", path.display()), true));
            false
        }
    }
}

/// Opens a graph file in place of the current graph.
fn open(st: &mut State) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Open a graph")
        .add_filter("Ridge graph", &[graph::FILE_EXTENSION])
        .pick_file()
    else {
        return;
    };
    match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| Graph::from_file(&text))
    {
        Ok(mut g) => {
            // graphs saved before they had names are named after their file
            if g.name.trim().is_empty() {
                g.name = path
                    .file_stem()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            }
            st.status = Some((format!("Opened {}", path.display()), false));
            st.replace(g, Some(path));
        }
        Err(e) => st.status = Some((format!("Couldn't open {}: {e}", path.display()), true)),
    }
}

/// New or Open: right away, or once the user said what to do with unsaved changes.
fn request(st: &mut State, what: Pending, ctx: &egui::Context) {
    if st.dirty {
        st.pending = Some(what);
    } else {
        run(st, what, ctx);
    }
}

fn run(st: &mut State, what: Pending, ctx: &egui::Context) {
    match what {
        Pending::New => {
            st.replace(Graph::default(), None);
            st.status = None;
        }
        Pending::Open => open(st),
        Pending::Close => {
            st.closing = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Every frame, whatever the tab: closing Ridge with unsaved graph changes asks first.
/// Whether the graph has changes not saved to its file.
pub(super) fn has_unsaved(app: &RidgeApp) -> bool {
    app.graph.dirty
}

pub(super) fn guard_close(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.graph;
    let asked = ui.ctx().input(|i| i.viewport().close_requested());
    if asked && st.dirty && !st.closing {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        st.pending = Some(Pending::Close);
    }
    unsaved_dialog(st, ui);
}

fn colour(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub(super) fn show(app: &mut RidgeApp, ui: &mut egui::Ui) {
    let st = &mut app.graph;
    // Ctrl+S saves
    // Ctrl+Shift+S first: Ctrl+S would match it too
    let save_as_key = egui::KeyboardShortcut::new(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::S,
    );
    let save_key = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
    if ui.input_mut(|i| i.consume_shortcut(&save_as_key)) {
        save(st, true);
    } else if ui.input_mut(|i| i.consume_shortcut(&save_key)) {
        save(st, false);
    }
    // V, H and L pick a tool, when no box is being typed in
    if ui.memory(|m| m.focused().is_none()) {
        for (key, tool) in [
            (egui::Key::V, Tool::Select),
            (egui::Key::H, Tool::Pan),
            (egui::Key::L, Tool::Link),
        ] {
            if ui.input(|i| i.key_pressed(key) && i.modifiers.is_none()) {
                st.tool = tool;
            }
        }
    }

    egui::Panel::left("graph_palette")
        .resizable(false)
        .exact_size(220.0)
        .show(ui, |ui| palette(st, ui));
    egui::Panel::right("graph_details")
        .resizable(true)
        .default_size(330.0)
        .min_size(260.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("graph_details")
                .auto_shrink(false)
                .show(ui, |ui| details(st, ui));
        });
    ui.add_space(4.0);
    // a little room between the toolbar and the palette's edge
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 8,
            right: 0,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| toolbar(st, ui));
    ui.add_space(4.0);
    canvas(st, ui);
    // the interactive layout: a step each frame; what is being dragged stays under the pointer
    if st.live_layout && !st.graph.nodes.is_empty() {
        let dragging = ui.input(|i| i.pointer.primary_down());
        let seen = (st.graph.nodes.len(), st.graph.links.len());
        // hot again after any change, then cooling until the graph rests
        if seen != st.live_seen {
            st.live_seen = seen;
            st.live_heat = 1.0;
        } else if dragging {
            st.live_heat = st.live_heat.max(0.6);
        } else {
            st.live_heat = (st.live_heat * 0.98).max(0.02);
        }
        let pinned = if dragging {
            st.selected.clone()
        } else {
            BTreeSet::new()
        };
        st.graph.settle_step(&st.sizes, &pinned, st.live_heat);
        ui.ctx().request_repaint();
    }
    // unsaved changes: compared when something happened, not every frame
    if ui.input(|i| !i.events.is_empty() || i.pointer.any_down()) {
        st.dirty = st.graph != st.saved;
    }
}

/// The entities by category: click one to add it, or drag it onto the board.
fn palette(st: &mut State, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::singleline(&mut st.search)
            .hint_text(icons::with(icons::SEARCH, "Search entities"))
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    let search = st.search.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("graph_kinds")
        .auto_shrink(false)
        .show(ui, |ui| {
            if !search.is_empty() {
                let found: Vec<&'static Kind> = KINDS
                    .iter()
                    .filter(|k| {
                        k.label.to_lowercase().contains(&search)
                            || k.category.to_lowercase().contains(&search)
                    })
                    .collect();
                if found.is_empty() {
                    ui.weak("No entity matches.");
                }
                kind_rows(st, ui, &found);
                return;
            }
            let recent: Vec<&'static Kind> = st
                .recent
                .iter()
                .filter_map(|key| KINDS.iter().find(|k| k.key == key))
                .collect();
            if !recent.is_empty() {
                egui::CollapsingHeader::new(egui::RichText::new("Recently used").strong())
                    .id_salt("graph_recent")
                    .default_open(true)
                    .show(ui, |ui| kind_rows(st, ui, &recent));
            }
            for category in CATEGORIES {
                let kinds: Vec<&'static Kind> =
                    KINDS.iter().filter(|k| k.category == *category).collect();
                egui::CollapsingHeader::new(egui::RichText::new(*category).strong())
                    .id_salt(("graph_category", *category))
                    .default_open(false)
                    .show(ui, |ui| kind_rows(st, ui, &kinds));
            }
        });
}

/// Palette entries: click one to add it, or drag it onto the board.
fn kind_rows(st: &mut State, ui: &mut egui::Ui, kinds: &[&'static Kind]) {
    for kind in kinds {
        // one area drawn by hand: labels would take the click as text selection
        let (rect, row) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 28.0),
            egui::Sense::click_and_drag(),
        );
        let row = row.on_hover_cursor(egui::CursorIcon::Grab);
        if row.hovered() {
            ui.painter()
                .rect_filled(rect, 4.0, Color32::from_white_alpha(12));
        }
        ui.painter().text(
            rect.left_center() + Vec2::new(14.0, 0.0),
            egui::Align2::CENTER_CENTER,
            kind.icon,
            egui::FontId::proportional(18.0),
            colour(kind.colour()),
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(34.0, 0.0),
            egui::Align2::LEFT_CENTER,
            kind.label,
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        if row.drag_started() {
            st.dragging_kind = Some(kind.key);
        }
        if row.clicked() {
            // near the top-left of what the board shows, beside the others
            let start = st.pan + Vec2::new(140.0, 80.0) / st.zoom;
            let (size, gap) = ([160.0, 60.0], 190.0);
            let columns = ((st.canvas_size.x / st.zoom - 140.0) / gap) as usize;
            let pos = st.graph.free_spot([start.x, start.y], size, columns);
            add_node(st, kind.key, pos);
        }
    }
}

fn add_node(st: &mut State, kind: &str, pos: [f32; 2]) {
    let pos = if st.snap { snapped(pos) } else { pos };
    let id = st.graph.add(kind, pos);
    st.recent.retain(|k| k != kind);
    st.recent.insert(0, kind.to_string());
    st.recent.truncate(RECENT);
    st.selected = BTreeSet::from([id]);
    st.selected_link = None;
}

fn snapped(pos: [f32; 2]) -> [f32; 2] {
    [
        (pos[0] / GRID).round() * GRID,
        (pos[1] / GRID).round() * GRID,
    ]
}

/// The graph's name and file buttons, the tools, align and spread, snapping, fit and zoom; then
/// a line saying what the last file action did.
fn toolbar(st: &mut State, ui: &mut egui::Ui) {
    let n = st.selected.len();
    // two fixed rows, so no button moves under the pointer
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.graph.name)
                .hint_text("Untitled graph")
                .desired_width(170.0),
        )
        .on_hover_text("Name of this graph");
        // the unsaved mark keeps its place even when hidden
        let mark = if st.dirty {
            ACCENT
        } else {
            Color32::TRANSPARENT
        };
        let mark = ui.label(egui::RichText::new("•").color(mark));
        if st.dirty {
            mark.on_hover_text("Unsaved changes");
        }
        if ui
            .button(icons::NEW_FILE)
            .on_hover_text("New graph")
            .clicked()
        {
            request(st, Pending::New, ui.ctx());
        }
        if ui
            .button(icons::FOLDER_OPEN)
            .on_hover_text("Open a graph…")
            .clicked()
        {
            request(st, Pending::Open, ui.ctx());
        }
        // one Save: right-click (or Ctrl+Shift+S) saves under another name
        let save_button = ui
            .button(icons::SAVE)
            .on_hover_text("Save (Ctrl+S) · right-click: Save as… (Ctrl+Shift+S)");
        if save_button.clicked() {
            save(st, false);
        }
        save_button.context_menu(|ui| {
            if ui.button(icons::with(icons::SAVE, "Save as…")).clicked() {
                ui.close();
                save(st, true);
            }
        });
        ui.separator();
        for (how, icon, tip) in [
            (
                Layout::Block,
                icons::LAYOUT_BLOCK,
                "Block layout: a grid, alike entities together",
            ),
            (
                Layout::Hierarchical,
                icons::LAYOUT_TREE,
                "Hierarchical layout: the most linked entity on top, what it links to below",
            ),
            (
                Layout::Circular,
                icons::LAYOUT_CIRCLE,
                "Circular layout: a ring, alike entities together",
            ),
            (
                Layout::Organic,
                icons::LAYOUT_ORGANIC,
                "Organic layout: linked entities pull together, the rest spread out",
            ),
        ] {
            let scope = if n >= 2 {
                "the selection"
            } else {
                "every entity"
            };
            if ui
                .button(icon)
                .on_hover_text(format!("{tip} (arranges {scope})"))
                .clicked()
            {
                let (selected, sizes) = (st.selected.clone(), st.sizes.clone());
                st.graph.layout(&selected, how, &sizes);
                if n < 2 {
                    st.zoom = 0.0; // show the whole result
                }
            }
        }
        if ui
            .toggle_value(&mut st.live_layout, icons::LAYOUT_LIVE)
            .on_hover_text("Interactive organic layout: keeps settling while you move entities")
            .clicked()
            && st.live_layout
        {
            st.live_heat = 1.0;
        }

        ui.separator();
        status_line(st, ui);
    });
    ui.horizontal(|ui| {
        for (tool, icon, tip) in [
            (
                Tool::Select,
                icons::POINTER,
                "Select (V): drag entities to move them, the background to select a box of them",
            ),
            (
                Tool::Pan,
                icons::HAND,
                "Pan (H): drag to move around the board",
            ),
            (
                Tool::Link,
                icons::LINK,
                "Link (L): drag from one entity onto another",
            ),
        ] {
            ui.selectable_value(&mut st.tool, tool, icon)
                .on_hover_text(tip);
        }

        ui.separator();
        ui.toggle_value(&mut st.snap, icons::GRID)
            .on_hover_text("Snap: entities settle on a grid when moved");
        if ui
            .button(icons::FIT)
            .on_hover_text("Fit: show every entity")
            .clicked()
        {
            st.zoom = 0.0; // fitted by the canvas, which knows its size
        }
        ui.weak(format!("{:.0}%", st.zoom * 100.0));
    });
}

/// What the last file action did, or where the graph is saved, on one line.
fn status_line(st: &State, ui: &mut egui::Ui) {
    let (text, error) = match &st.status {
        Some((text, error)) => (text.clone(), *error),
        None => (
            st.path
                .as_ref()
                .map_or("Not saved yet".into(), |p| p.display().to_string()),
            false,
        ),
    };
    let colour = if error {
        Color32::from_rgb(218, 54, 51)
    } else {
        ui.visuals().weak_text_color()
    };
    ui.add(
        egui::Label::new(egui::RichText::new(&text).small().color(colour))
            .truncate()
            .selectable(false),
    )
    .on_hover_text(&text);
}

/// Turns board positions into screen positions and back.
#[derive(Clone, Copy)]
struct View {
    origin: Pos2,
    pan: Vec2,
    zoom: f32,
}

impl View {
    fn to_screen(self, p: [f32; 2]) -> Pos2 {
        self.origin + (Vec2::new(p[0], p[1]) - self.pan) * self.zoom
    }

    fn to_board(self, p: Pos2) -> [f32; 2] {
        let b = (p - self.origin) / self.zoom + self.pan;
        [b.x, b.y]
    }
}

fn canvas(st: &mut State, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    st.canvas_size = rect.size();
    let bg = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    // "Fit" (or a first look): every entity in view
    if st.zoom == 0.0 {
        fit(st, rect);
    }

    // zoom around the pointer with Ctrl+scroll or a pinch; scroll pans
    if bg.hovered() || bg.contains_pointer() {
        let (zoom_delta, scroll, pointer) =
            ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        if zoom_delta != 1.0 {
            if let Some(p) = pointer {
                let before = View {
                    origin: rect.min,
                    pan: st.pan,
                    zoom: st.zoom,
                }
                .to_board(p);
                st.zoom = (st.zoom * zoom_delta).clamp(0.25, 3.0);
                st.pan = Vec2::new(before[0], before[1]) - (p - rect.min) / st.zoom;
            }
        } else if scroll != Vec2::ZERO {
            st.pan -= scroll / st.zoom;
        }
    }
    let view = View {
        origin: rect.min,
        pan: st.pan,
        zoom: st.zoom,
    };

    // dots of the grid
    let step = GRID * view.zoom * if view.zoom < 0.6 { 4.0 } else { 1.0 };
    if step > 6.0 {
        let start = view.to_screen([
            (st.pan.x / GRID).floor() * GRID,
            (st.pan.y / GRID).floor() * GRID,
        ]);
        let dot = Color32::from_white_alpha(14);
        let mut y = start.y;
        while y < rect.bottom() {
            let mut x = start.x;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 1.0, dot);
                x += step;
            }
            y += step;
        }
    }

    // every entity's picture, ready before the graph is borrowed for drawing
    for n in &st.graph.nodes {
        picture_texture(&mut st.pictures, ui.ctx(), n);
    }
    let g = &st.graph;
    if g.nodes.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Add entities from the left to start this graph.",
            egui::FontId::proportional(15.0),
            ui.visuals().weak_text_color(),
        );
    }

    // each entity's box on screen
    let z = view.zoom;
    let title_font = egui::FontId::proportional(13.5 * z);
    let sub_font = egui::FontId::proportional(11.0 * z);
    let icon_font = egui::FontId::proportional(20.0 * z);
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    struct Drawn {
        id: u64,
        rect: Rect,
        title: std::sync::Arc<egui::Galley>,
        sub: Option<std::sync::Arc<egui::Galley>>,
        icon: &'static str,
        colour: Color32,
        picture: Option<egui::TextureId>,
        border: LineStyle,
    }
    let drawn: Vec<Drawn> = g
        .nodes
        .iter()
        .map(|n| {
            let kind = graph::kind(&n.kind);
            let short = |s: String, max: usize| {
                if s.chars().count() > max {
                    format!("{}…", s.chars().take(max - 1).collect::<String>())
                } else {
                    s
                }
            };
            let title = painter.layout_no_wrap(short(n.title(), 30), title_font.clone(), text);
            let sub = Some(n.subtitle())
                .filter(|s| !s.is_empty())
                .map(|s| painter.layout_no_wrap(short(s, 36), sub_font.clone(), weak));
            let text_w = title.size().x.max(sub.as_ref().map_or(0.0, |s| s.size().x));
            let size = Vec2::new(
                (text_w + 52.0 * z).max(120.0 * z),
                if sub.is_some() { 48.0 } else { 36.0 } * z,
            );
            Drawn {
                id: n.id,
                rect: Rect::from_center_size(view.to_screen(n.pos), size),
                title,
                sub,
                icon: kind.icon,
                colour: colour(kind.colour()),
                border: n.border,
                picture: st
                    .pictures
                    .get(&n.id)
                    .and_then(|(_, t)| t.as_ref().map(|t| t.id())),
            }
        })
        .collect();
    st.sizes = drawn
        .iter()
        .map(|d| (d.id, [d.rect.width() / z, d.rect.height() / z]))
        .collect();
    let rect_of: BTreeMap<u64, Rect> = drawn.iter().map(|d| (d.id, d.rect)).collect();

    // links under the entities, each with its label (or a dot) in the middle to select it
    let mut link_hits = Vec::new();
    for (i, link) in g.links.iter().enumerate() {
        let (Some(a), Some(b)) = (rect_of.get(&link.from), rect_of.get(&link.to)) else {
            continue;
        };
        let selected = st.selected_link == Some(i);
        let stroke = Stroke::new(
            if selected { 2.5 } else { 1.5 } * z.max(0.6),
            if selected {
                ACCENT
            } else {
                Color32::from_gray(120)
            },
        );
        // from edge to edge, so arrowheads touch the boxes
        let (start, end) = (edge_point(*a, b.center()), edge_point(*b, a.center()));
        styled_line(&painter, &[start, end], stroke, link.style, z);
        let head = 10.0 * z.max(0.6);
        if link.arrow.to_end() {
            arrowhead(&painter, end, end - start, head, stroke.color);
        }
        if link.arrow.to_start() {
            arrowhead(&painter, start, start - end, head, stroke.color);
        }
        let mid = start + (end - start) / 2.0;
        let hit = if link.label.trim().is_empty() {
            painter.circle_filled(mid, 4.0 * z.max(0.7), stroke.color);
            Rect::from_center_size(mid, Vec2::splat(14.0))
        } else {
            let galley = painter.layout_no_wrap(
                link.label.clone(),
                egui::FontId::proportional(11.0 * z),
                text,
            );
            let r = Rect::from_center_size(mid, galley.size() + Vec2::new(10.0, 4.0) * z);
            painter.rect_filled(r, 4.0, ui.visuals().panel_fill);
            painter.rect_stroke(r, 4.0, stroke, egui::StrokeKind::Outside);
            painter.galley(r.center() - galley.size() / 2.0, galley, text);
            r
        };
        link_hits.push((i, hit));
    }

    // the link being drawn
    let pointer = ui.input(|i| i.pointer.interact_pos());
    if let (Some(from), Some(p)) = (st.linking.and_then(|id| rect_of.get(&id)), pointer) {
        painter.line_segment([from.center(), p], Stroke::new(2.0, ACCENT));
    }

    for d in &drawn {
        let selected = st.selected.contains(&d.id);
        painter.rect_filled(d.rect, 8.0 * z, ui.visuals().panel_fill);
        let border = if selected {
            Stroke::new(2.0, ACCENT)
        } else {
            Stroke::new(1.5, d.colour)
        };
        if d.border == LineStyle::Solid {
            painter.rect_stroke(d.rect, 8.0 * z, border, egui::StrokeKind::Inside);
        } else {
            let r = d.rect.shrink(border.width / 2.0);
            let outline = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
                r.left_top(),
            ];
            styled_line(&painter, &outline, border, d.border, z);
        }
        let left = d.rect.left() + 10.0 * z;
        if let Some(tex) = d.picture {
            // its picture in place of the icon
            let side = (d.rect.height() - 12.0 * z).min(26.0 * z);
            let spot = Rect::from_min_size(
                Pos2::new(left - 2.0 * z, d.rect.center().y - side / 2.0),
                Vec2::splat(side),
            );
            egui::Image::new((tex, spot.size()))
                .corner_radius(4)
                .paint_at(ui, spot);
        } else {
            painter.text(
                Pos2::new(left, d.rect.center().y),
                egui::Align2::LEFT_CENTER,
                d.icon,
                icon_font.clone(),
                d.colour,
            );
        }
        let text_x = left + 30.0 * z;
        let total = d.title.size().y + d.sub.as_ref().map_or(0.0, |s| s.size().y);
        let mut y = d.rect.center().y - total / 2.0;
        painter.galley(Pos2::new(text_x, y), d.title.clone(), text);
        y += d.title.size().y;
        if let Some(sub) = &d.sub {
            painter.galley(Pos2::new(text_x, y), sub.clone(), weak);
        }
        // the handle to draw a link from
        if selected || ui.rect_contains_pointer(d.rect.expand(8.0)) {
            painter.circle(
                handle_pos(d.rect),
                5.0 * z.max(0.7),
                ui.visuals().panel_fill,
                Stroke::new(1.5, ACCENT),
            );
        }
    }

    // input: links, then entities, then their handles (the last one added is on top)
    let modifiers = ui.input(|i| i.modifiers);
    for (i, hit) in link_hits {
        let r = ui.interact(
            hit,
            ui.scope_id().with(("graph_link", i)),
            egui::Sense::click(),
        );
        if r.clicked() {
            st.selected.clear();
            st.selected_link = Some(i);
        }
    }
    let tool = st.tool;
    let primary = egui::PointerButton::Primary;
    let middle = egui::PointerButton::Middle;
    let holding = ui.input(|i| i.pointer.primary_down());
    let mut moved = Vec2::ZERO;
    let mut panned = Vec2::ZERO;
    let mut drag_ended = false;
    for d in &drawn {
        let r = ui
            .interact(
                d.rect,
                ui.scope_id().with(("graph_node", d.id)),
                egui::Sense::click_and_drag(),
            )
            .on_hover_cursor(match tool {
                Tool::Pan if holding => egui::CursorIcon::Grabbing,
                Tool::Select | Tool::Pan => egui::CursorIcon::Grab,
                Tool::Link => egui::CursorIcon::Crosshair,
            });
        // the middle button pans whatever the tool; Pan pans from entities too
        if r.dragged_by(middle) || (tool == Tool::Pan && r.dragged_by(primary)) {
            panned += r.drag_delta();
            continue;
        }
        if tool == Tool::Link {
            if r.drag_started_by(primary) {
                st.linking = Some(d.id);
            }
            if r.clicked() {
                st.selected = BTreeSet::from([d.id]);
                st.selected_link = None;
            }
            continue;
        }
        if r.clicked() || r.drag_started_by(primary) {
            st.selected_link = None;
            if modifiers.command || modifiers.shift {
                if r.clicked() && !st.selected.insert(d.id) {
                    st.selected.remove(&d.id);
                }
                if r.drag_started_by(primary) {
                    st.selected.insert(d.id);
                }
            } else if !st.selected.contains(&d.id) || r.clicked() {
                st.selected = BTreeSet::from([d.id]);
            }
        }
        if r.dragged_by(primary) {
            moved += r.drag_delta() / z;
        }
        drag_ended |= r.drag_stopped();
        if tool != Tool::Select {
            continue;
        }

        let h = ui.interact(
            Rect::from_center_size(handle_pos(d.rect), Vec2::splat(14.0)),
            ui.scope_id().with(("graph_handle", d.id)),
            egui::Sense::drag(),
        );
        if h.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if h.drag_started() {
            st.linking = Some(d.id);
        }
    }
    let snap = st.snap;
    let selected = st.selected.clone();
    if moved != Vec2::ZERO {
        for n in st
            .graph
            .nodes
            .iter_mut()
            .filter(|n| selected.contains(&n.id))
        {
            n.pos[0] += moved.x;
            n.pos[1] += moved.y;
        }
    }
    if drag_ended && snap {
        for n in st
            .graph
            .nodes
            .iter_mut()
            .filter(|n| selected.contains(&n.id))
        {
            n.pos = snapped(n.pos);
        }
    }

    // a link dropped on another entity
    let released = ui.input(|i| i.pointer.any_released());
    if released {
        if let (Some(from), Some(p)) = (st.linking.take(), pointer) {
            let target = drawn
                .iter()
                .rev()
                .find(|d| d.rect.contains(p))
                .map(|d| d.id);
            if let Some(to) = target {
                if st.graph.link(from, to) {
                    st.selected.clear();
                    st.selected_link = Some(st.graph.links.len() - 1);
                }
            }
        }
    }

    // a palette entry dropped on the board
    if let Some(kind) = st.dragging_kind {
        if let Some(p) = pointer {
            painter.text(
                p,
                egui::Align2::CENTER_CENTER,
                graph::kind(kind).icon,
                egui::FontId::proportional(26.0),
                ACCENT,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if released {
            st.dragging_kind = None;
            if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
                add_node(st, kind, view.to_board(p));
            }
        }
    }

    // the background: click clears; drag selects a box (Select, or Shift) or pans (Pan, or the
    // middle button)
    if bg.clicked() {
        st.clear_selection();
    }
    let boxes = tool == Tool::Select || modifiers.shift;
    if bg.drag_started_by(primary) && boxes {
        st.boxing = pointer;
    }
    if (bg.dragged_by(primary) && st.boxing.is_none()) || bg.dragged_by(middle) {
        panned += bg.drag_delta();
    }
    if tool == Tool::Pan && bg.hovered() {
        ui.ctx().set_cursor_icon(if holding {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }
    st.pan -= panned / z;
    if let (Some(start), Some(p)) = (st.boxing, pointer) {
        let area = Rect::from_two_pos(start, p);
        painter.rect(
            area,
            0.0,
            ACCENT.gamma_multiply(0.08),
            Stroke::new(1.0, ACCENT),
            egui::StrokeKind::Inside,
        );
        if bg.drag_stopped() || released {
            st.boxing = None;
            if !modifiers.command {
                st.selected.clear();
            }
            st.selected_link = None;
            st.selected.extend(
                drawn
                    .iter()
                    .filter(|d| area.intersects(d.rect))
                    .map(|d| d.id),
            );
        }
    }

    // keys, when no text box is being typed in
    if ui.memory(|m| m.focused().is_none()) && bg.contains_pointer() {
        let (delete, all) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::A),
            )
        });
        if delete {
            delete_selection(st);
        }
        if all {
            st.selected = drawn.iter().map(|d| d.id).collect();
        }
    }
}

/// A line through `points` drawn solid, dotted or dashed.
fn styled_line(painter: &egui::Painter, points: &[Pos2], stroke: Stroke, style: LineStyle, z: f32) {
    let z = z.max(0.5);
    match style {
        LineStyle::Solid => {
            painter.add(egui::Shape::line(points.to_vec(), stroke));
        }
        LineStyle::Dotted => painter.extend(egui::Shape::dotted_line(
            points,
            stroke.color,
            stroke.width * 2.0 + 2.0 * z,
            stroke.width * 0.6,
        )),
        LineStyle::ShortDashes => {
            painter.extend(egui::Shape::dashed_line(points, stroke, 4.0 * z, 3.0 * z))
        }
        LineStyle::LongDashes => {
            painter.extend(egui::Shape::dashed_line(points, stroke, 12.0 * z, 5.0 * z))
        }
    }
}

/// Where the line from `rect`'s centre towards `toward` leaves the rectangle.
fn edge_point(rect: Rect, toward: Pos2) -> Pos2 {
    let c = rect.center();
    let d = toward - c;
    let tx = if d.x == 0.0 {
        f32::INFINITY
    } else {
        rect.width() / 2.0 / d.x.abs()
    };
    let ty = if d.y == 0.0 {
        f32::INFINITY
    } else {
        rect.height() / 2.0 / d.y.abs()
    };
    let t = tx.min(ty).min(1.0);
    if t.is_finite() {
        c + d * t
    } else {
        c
    }
}

/// A filled arrowhead with its tip at `tip`, pointing along `direction`.
fn arrowhead(painter: &egui::Painter, tip: Pos2, direction: Vec2, size: f32, colour: Color32) {
    let dir = direction.normalized();
    if !dir.is_finite() {
        return;
    }
    let back = tip - dir * size;
    let side = dir.rot90() * size * 0.5;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, back + side, back - side],
        colour,
        Stroke::NONE,
    ));
}

/// Line-style picker; returns the chosen style when it changed.
fn style_picker(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    current: Option<LineStyle>,
) -> Option<LineStyle> {
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.map_or("Mixed", LineStyle::label))
        .show_ui(ui, |ui| {
            for style in LineStyle::ALL {
                if ui
                    .selectable_label(current == Some(style), style.label())
                    .clicked()
                {
                    chosen = Some(style);
                }
            }
        });
    chosen.filter(|s| Some(*s) != current)
}

fn handle_pos(rect: Rect) -> Pos2 {
    Pos2::new(rect.right(), rect.center().y)
}

/// Zoom and pan so every entity shows (or the default view for an empty graph).
fn fit(st: &mut State, rect: Rect) {
    st.zoom = 1.0;
    st.pan = Vec2::ZERO;
    let g = &st.graph;
    if g.nodes.is_empty() {
        return;
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for n in &g.nodes {
        let half = st
            .sizes
            .get(&n.id)
            .map_or(Vec2::new(70.0, 24.0), |s| Vec2::new(s[0], s[1]) / 2.0);
        let p = Vec2::new(n.pos[0], n.pos[1]);
        min = min.min(p - half);
        max = max.max(p + half);
    }
    let margin = Vec2::splat(60.0);
    let size = max - min + margin * 2.0;
    st.zoom = (rect.width() / size.x)
        .min(rect.height() / size.y)
        .clamp(0.25, 1.5);
    let centre = (min + max) / 2.0;
    st.pan = centre - rect.size() / 2.0 / st.zoom;
}

fn delete_selection(st: &mut State) {
    let selected = std::mem::take(&mut st.selected);
    let link = st.selected_link.take();
    let g = &mut st.graph;
    if !selected.is_empty() {
        g.remove(&selected);
    } else if let Some(i) = link.filter(|i| *i < g.links.len()) {
        g.links.remove(i);
    }
}

/// The right panel: the selected entity's facts, notes and links; or the selected link; or
/// what to do with several.
fn details(st: &mut State, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    if let Some(i) = st.selected_link {
        link_details(st, ui, i);
        return;
    }
    match st.selected.len() {
        0 => {
            ui.heading("Details");
            ui.weak("Select an entity to fill in its facts and notes, or a link to name it.");
        }
        1 => {
            let id = *st.selected.iter().next().unwrap_or(&0);
            node_details(st, ui, id);
        }
        n => {
            ui.heading(format!("{n} entities selected"));
            ui.weak("Arrange them with the layout buttons above the board.");
            ui.add_space(8.0);
            let styles: BTreeSet<_> = st
                .graph
                .nodes
                .iter()
                .filter(|n| st.selected.contains(&n.id))
                .map(|n| n.border as u8)
                .collect();
            let current = st
                .graph
                .nodes
                .iter()
                .find(|n| st.selected.contains(&n.id))
                .map(|n| n.border)
                .filter(|_| styles.len() == 1);
            ui.horizontal(|ui| {
                ui.label("Border");
                if let Some(style) = style_picker(ui, "graph_border_many", current) {
                    for node in st
                        .graph
                        .nodes
                        .iter_mut()
                        .filter(|n| st.selected.contains(&n.id))
                    {
                        node.border = style;
                    }
                }
            });
            ui.add_space(8.0);
            if ui
                .button(icons::with(icons::DELETE, &format!("Delete {n} entities")))
                .clicked()
            {
                delete_selection(st);
            }
        }
    }
}

fn node_details(st: &mut State, ui: &mut egui::Ui, id: u64) {
    if let Some(n) = st.graph.node(id) {
        picture_texture(&mut st.pictures, ui.ctx(), n);
    }
    let picture = st.pictures.get(&id).and_then(|(_, t)| t.clone());
    let mut picture_error = None;
    let g = &mut st.graph;
    // titles of the other entities, for the links list
    let titles: BTreeMap<u64, String> = g.nodes.iter().map(|n| (n.id, n.title())).collect();
    let Some(node) = g.node_mut(id) else {
        st.selected.clear();
        return;
    };
    let kind = graph::kind(&node.kind);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(kind.icon)
                .size(22.0)
                .color(colour(kind.colour())),
        );
        ui.heading(kind.label);
    });
    ui.horizontal(|ui| {
        ui.label("Border");
        if let Some(style) = style_picker(ui, ("graph_border", id), Some(node.border)) {
            node.border = style;
        }
    });
    ui.add_space(6.0);
    egui::Grid::new(("graph_fields", id))
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for field in kind.fields {
                ui.label(field.label);
                ui.horizontal(|ui| {
                    let value = node.values.entry(field.key.to_string()).or_default();
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text(field.hint)
                            .desired_width(170.0),
                    );
                    if !field.presets.is_empty() {
                        ui.menu_button("▾", |ui| {
                            for preset in field.presets {
                                if ui.button(*preset).clicked() {
                                    *value = preset.to_string();
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                ui.end_row();
            }
        });
    node.values.retain(|_, v| !v.is_empty());

    ui.add_space(10.0);
    ui.strong("Picture");
    if let Some(tex) = &picture {
        ui.add(
            egui::Image::new((tex.id(), tex.size_vec2()))
                .max_size(Vec2::new(ui.available_width(), 220.0))
                .corner_radius(6),
        )
        .on_hover_text(&node.picture_name);
    } else if !node.picture.is_empty() {
        ui.colored_label(
            Color32::from_rgb(218, 54, 51),
            "This picture can't be shown",
        );
    }
    ui.horizontal(|ui| {
        if ui
            .button(icons::with(icons::IMAGE, "Choose picture…"))
            .on_hover_text("Kept inside the graph file")
            .clicked()
        {
            if let Some(file) = rfd::FileDialog::new()
                .set_title("Choose a picture of this entity")
                .add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif"])
                .pick_file()
            {
                let name = file
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                let result = std::fs::read(&file)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| node.set_picture(&name, &bytes));
                if let Err(e) = result {
                    picture_error = Some(e);
                }
            }
        }
        if !node.picture.is_empty() && ui.button(icons::with(icons::CANCEL, "Remove")).clicked() {
            node.picture.clear();
            node.picture_name.clear();
        }
    });
    if let Some(e) = &picture_error {
        ui.colored_label(Color32::from_rgb(218, 54, 51), e);
    }

    ui.add_space(10.0);
    ui.strong("Notes");
    ui.add(
        egui::TextEdit::multiline(&mut node.notes)
            .desired_width(f32::INFINITY)
            .desired_rows(14)
            .hint_text("Sources, how it was found, what to check next…"),
    );

    ui.add_space(10.0);
    ui.strong("Links");
    let mut remove = None;
    let mut any = false;
    for (i, link) in g.links.iter_mut().enumerate() {
        let other = if link.from == id {
            link.to
        } else if link.to == id {
            link.from
        } else {
            continue;
        };
        any = true;
        ui.horizontal(|ui| {
            link_label_edit(ui, &mut link.label, ("graph_link_label", i), 110.0);
            ui.label(titles.get(&other).cloned().unwrap_or_default());
            if ui
                .small_button(icons::DELETE)
                .on_hover_text("Remove link")
                .clicked()
            {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        g.links.remove(i);
    }
    if !any {
        ui.weak("Drag the ● on its right edge onto another entity.");
    }

    ui.add_space(12.0);
    if ui
        .button(icons::with(icons::DELETE, "Delete entity"))
        .clicked()
    {
        delete_selection(st);
    }
}

fn link_details(st: &mut State, ui: &mut egui::Ui, i: usize) {
    let g = &mut st.graph;
    let title = |id: u64| g.node(id).map(|n| n.title()).unwrap_or_default();
    let Some((from, to)) = g.links.get(i).map(|l| (title(l.from), title(l.to))) else {
        st.selected_link = None;
        return;
    };
    ui.heading("Link");
    ui.label(format!("{from}  —  {to}"));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Label");
        link_label_edit(ui, &mut g.links[i].label, ("graph_link_detail", i), 180.0);
    });
    let link = &mut g.links[i];
    ui.horizontal(|ui| {
        ui.label("Line");
        if let Some(style) = style_picker(ui, ("graph_link_style", i), Some(link.style)) {
            link.style = style;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Arrow");
        for arrow in Arrow::ALL {
            let tip = match arrow {
                Arrow::None => "No arrow".to_string(),
                Arrow::Forward => format!("Towards {to}"),
                Arrow::Backward => format!("Towards {from}"),
                Arrow::Both => "Both ways".to_string(),
            };
            ui.selectable_value(&mut link.arrow, arrow, arrow.symbol())
                .on_hover_text(tip);
        }
    });
    ui.add_space(12.0);
    if ui
        .button(icons::with(icons::DELETE, "Remove link"))
        .clicked()
    {
        delete_selection(st);
    }
}

fn link_label_edit(ui: &mut egui::Ui, label: &mut String, id: impl egui::AsIdSalt, width: f32) {
    ui.push_id(id, |ui| {
        ui.add(
            egui::TextEdit::singleline(label)
                .hint_text("owns, uses…")
                .desired_width(width),
        );
        ui.menu_button("▾", |ui| {
            for preset in LINK_LABELS {
                if ui.button(*preset).clicked() {
                    *label = preset.to_string();
                    ui.close();
                }
            }
        });
    });
}

/// "Save changes?" before New or Open replaces a graph with unsaved changes.
fn unsaved_dialog(st: &mut State, ui: &mut egui::Ui) {
    let Some(what) = st.pending else {
        return;
    };
    let title = st.title();
    let (mut save_first, mut discard, mut cancel) = (false, false, false);
    egui::Modal::new(egui::Id::unique("graph_unsaved")).show(ui.ctx(), |ui| {
        ui.set_width(400.0);
        ui.heading(format!("Save changes to {title}?"));
        ui.label(if what == Pending::Close {
            "Ridge opens on a blank graph next time: unsaved changes are lost."
        } else {
            "Otherwise they are lost."
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            save_first = ui.button(icons::with(icons::SAVE, "Save")).clicked();
            discard = ui.button("Don't save").clicked();
            cancel = ui.button(icons::with(icons::CANCEL, "Cancel")).clicked();
        });
    });
    if save_first || discard || cancel {
        st.pending = None;
    }
    if (save_first && save(st, false)) || discard {
        run(st, what, ui.ctx());
    }
}

/// The entity's picture as a texture (made again when its picture changes).
fn picture_texture(
    cache: &mut HashMap<u64, (usize, Option<egui::TextureHandle>)>,
    ctx: &egui::Context,
    node: &graph::Node,
) {
    if node.picture.is_empty() {
        cache.remove(&node.id);
        return;
    }
    if cache
        .get(&node.id)
        .is_some_and(|(len, _)| *len == node.picture.len())
    {
        return;
    }
    let texture = node
        .picture_bytes()
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(|img| {
            let img = img.thumbnail(320, 320).to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            let colour = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            ctx.load_texture(
                format!("graph_picture_{}", node.id),
                colour,
                egui::TextureOptions::LINEAR,
            )
        });
    cache.insert(node.id, (node.picture.len(), texture));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_end_on_the_box_edges() {
        let near = |p: Pos2, x: f32, y: f32| (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3;
        let r = Rect::from_center_size(Pos2::new(0.0, 0.0), Vec2::new(100.0, 40.0));
        // straight right: the right edge; straight down: the bottom edge
        assert!(near(edge_point(r, Pos2::new(300.0, 0.0)), 50.0, 0.0));
        assert!(near(edge_point(r, Pos2::new(0.0, 300.0)), 0.0, 20.0));
        // diagonally: whichever edge comes first (here the bottom)
        assert!(near(edge_point(r, Pos2::new(100.0, 100.0)), 20.0, 20.0));
        // a point inside the box, or its centre, stays put
        assert!(near(edge_point(r, Pos2::new(10.0, 5.0)), 10.0, 5.0));
        assert!(near(edge_point(r, r.center()), 0.0, 0.0));
    }
}
