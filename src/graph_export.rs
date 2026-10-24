//! Renders an investigation graph (see `graph`) to a flat picture, for sharing outside Ridge.
//! Draws the same boxes, links and captions as the board, straight onto an `image::RgbaImage`,
//! so no window or GPU context is needed.

use ab_glyph::FontRef;
use image::{Rgba, RgbaImage};

use crate::annotate::{draw_text, fill, text_width};
use crate::graph::{self, Graph, LineStyle, Node};

/// Scales the board's own units up for a crisper picture than the on-screen zoom-1 size.
const SCALE: f32 = 2.25;
const MARGIN: f32 = 60.0;
const TITLE_SIZE: f32 = 14.0;
const SUB_SIZE: f32 = 11.5;
const SOURCE_SIZE: f32 = 10.5;
const ICON_SIZE: f32 = 20.0;
const MIN_WIDTH: f32 = 130.0;
const LINK_STROKE: f32 = 1.6;
const ARROW_SIZE: f32 = 10.0;
/// So very large graphs still export to a picture that fits in memory and on disk.
const MAX_SIDE: u32 = 9000;

type P = [f32; 2];

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: P, s: f32) -> P {
    [a[0] * s, a[1] * s]
}
fn length(a: P) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}
fn normalized(a: P) -> P {
    let l = length(a);
    if l > 0.0 {
        mul(a, 1.0 / l)
    } else {
        [0.0, 0.0]
    }
}
fn rot90(a: P) -> P {
    [-a[1], a[0]]
}

/// A box on the board, in board units (its own, not yet placed in the picture).
struct Size {
    w: f32,
    h: f32,
    has_sub: bool,
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

fn node_size(node: &Node, title_font: &FontRef, sub_font: &FontRef) -> Size {
    let title_w = text_width(title_font, &short(&node.title(), 30), TITLE_SIZE);
    let subtitle = node.subtitle();
    let has_sub = !subtitle.is_empty();
    let sub_w = if has_sub {
        text_width(sub_font, &short(&subtitle, 36), SUB_SIZE)
    } else {
        0.0
    };
    Size {
        w: (title_w.max(sub_w) + 52.0).max(MIN_WIDTH),
        h: if has_sub { 48.0 } else { 36.0 },
        has_sub,
    }
}

/// Where the line from a box centred at `centre` (half-size `half`) towards `toward` leaves it.
fn edge_point(centre: P, half: P, toward: P) -> P {
    let d = sub(toward, centre);
    let tx = if d[0] == 0.0 {
        f32::INFINITY
    } else {
        half[0] / d[0].abs()
    };
    let ty = if d[1] == 0.0 {
        f32::INFINITY
    } else {
        half[1] / d[1].abs()
    };
    let t = tx.min(ty).min(1.0);
    if t.is_finite() {
        add(centre, mul(d, t))
    } else {
        centre
    }
}

fn fill_square(img: &mut RgbaImage, centre: P, side: f32, colour: Rgba<u8>) {
    let h = side / 2.0;
    fill(
        img,
        [centre[0] - h, centre[1] - h, centre[0] + h, centre[1] + h],
        colour,
    );
}

/// A line, `stroke` wide, from `a` to `b`, solid, dotted or dashed.
fn draw_line(img: &mut RgbaImage, a: P, b: P, stroke: f32, colour: Rgba<u8>, style: LineStyle) {
    let d = sub(b, a);
    let total = length(d);
    if total < 0.01 {
        return;
    }
    let dir = normalized(d);
    let (dash, gap) = match style {
        LineStyle::Solid => (total + 1.0, 0.0),
        LineStyle::Dotted => (stroke, stroke * 1.8),
        LineStyle::ShortDashes => (stroke * 3.5, stroke * 2.2),
        LineStyle::LongDashes => (stroke * 8.0, stroke * 4.5),
    };
    let mut t = 0.0;
    loop {
        let seg_end = (t + dash).min(total);
        let (p0, p1) = (add(a, mul(dir, t)), add(a, mul(dir, seg_end)));
        let seg_len = length(sub(p1, p0));
        let steps = ((seg_len / (stroke * 0.5).max(1.0)).ceil() as i32).max(1);
        for i in 0..=steps {
            let f = i as f32 / steps as f32;
            fill_square(img, add(p0, mul(sub(p1, p0), f)), stroke, colour);
        }
        t += dash + gap.max(0.01);
        if gap <= 0.0 || t >= total {
            break;
        }
    }
}

/// A box outline, `stroke` wide, from its centre and half-size.
fn draw_box_outline(
    img: &mut RgbaImage,
    centre: P,
    half: P,
    stroke: f32,
    colour: Rgba<u8>,
    style: LineStyle,
) {
    let tl = sub(centre, half);
    let tr = [centre[0] + half[0], centre[1] - half[1]];
    let br = add(centre, half);
    let bl = [centre[0] - half[0], centre[1] + half[1]];
    for (p, q) in [(tl, tr), (tr, br), (br, bl), (bl, tl)] {
        draw_line(img, p, q, stroke, colour, style);
    }
}

fn fill_triangle(img: &mut RgbaImage, p0: P, p1: P, p2: P, colour: Rgba<u8>) {
    let (w, h) = img.dimensions();
    let min_x = p0[0].min(p1[0]).min(p2[0]).floor().max(0.0) as u32;
    let max_x = (p0[0].max(p1[0]).max(p2[0]).ceil().max(0.0) as u32).min(w);
    let min_y = p0[1].min(p1[1]).min(p2[1]).floor().max(0.0) as u32;
    let max_y = (p0[1].max(p1[1]).max(p2[1]).ceil().max(0.0) as u32).min(h);
    let sign = |a: P, b: P, c: P| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let (d1, d2, d3) = (sign(p0, p1, p), sign(p1, p2, p), sign(p2, p0, p));
            let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            if !(neg && pos) {
                img.put_pixel(x, y, colour);
            }
        }
    }
}

fn arrowhead(img: &mut RgbaImage, tip: P, direction: P, size: f32, colour: Rgba<u8>) {
    let dir = normalized(direction);
    if dir == [0.0, 0.0] {
        return;
    }
    let back = sub(tip, mul(dir, size));
    let side = mul(rot90(dir), size * 0.5);
    fill_triangle(img, tip, add(back, side), sub(back, side), colour);
}

fn centred_text(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    centre_x: f32,
    baseline: f32,
    size: f32,
    colour: Rgba<u8>,
) {
    let w = text_width(font, text, size);
    draw_text(img, font, text, centre_x - w / 2.0, baseline, size, colour);
}

/// The graph, drawn flat: boxes with their icon or picture, title, subtitle and source, and the
/// links between them with their labels and arrows.
pub fn render(graph: &Graph) -> Result<RgbaImage, String> {
    let title_font = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT)
        .map_err(|e| format!("title font: {e}"))?;
    let sub_font = title_font.clone();
    let icon_font = FontRef::try_from_slice(include_bytes!("../assets/Phosphor.ttf"))
        .map_err(|e| format!("icon font: {e}"))?;

    if graph.nodes.is_empty() {
        return Ok(RgbaImage::from_pixel(640, 360, Rgba([255, 255, 255, 255])));
    }

    let sizes: Vec<(&Node, Size, bool)> = graph
        .nodes
        .iter()
        .map(|n| {
            let size = node_size(n, &title_font, &sub_font);
            let has_source = !n.source.trim().is_empty();
            (n, size, has_source)
        })
        .collect();

    // the board's bounding box, in board units, each box's extent plus its source caption
    let mut min = [f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY];
    for (n, size, has_source) in &sizes {
        let half = [size.w / 2.0, size.h / 2.0];
        let bottom_extra = if *has_source { 20.0 } else { 0.0 };
        min = [
            min[0].min(n.pos[0] - half[0]),
            min[1].min(n.pos[1] - half[1]),
        ];
        max = [
            max[0].max(n.pos[0] + half[0]),
            max[1].max(n.pos[1] + half[1] + bottom_extra),
        ];
    }
    min = [min[0] - MARGIN, min[1] - MARGIN];
    max = [max[0] + MARGIN, max[1] + MARGIN];
    let board_size = sub(max, min);

    let mut scale = SCALE;
    let longest = board_size[0].max(board_size[1]) * scale;
    if longest > MAX_SIDE as f32 {
        scale *= MAX_SIDE as f32 / longest;
    }
    let to_px = |p: P| mul(sub(p, min), scale);

    let (w, h) = (
        (board_size[0] * scale).ceil().max(1.0) as u32,
        (board_size[1] * scale).ceil().max(1.0) as u32,
    );
    let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));

    let boxes: std::collections::BTreeMap<u64, (P, P)> = sizes
        .iter()
        .map(|(n, size, _)| {
            (
                n.id,
                (to_px(n.pos), mul([size.w / 2.0, size.h / 2.0], scale)),
            )
        })
        .collect();

    // links, under the boxes
    for link in &graph.links {
        let (Some(&(ca, ha)), Some(&(cb, hb))) = (boxes.get(&link.from), boxes.get(&link.to))
        else {
            continue;
        };
        let stroke = LINK_STROKE * scale;
        let colour = Rgba([120, 120, 120, 255]);
        let (start, end) = (edge_point(ca, ha, cb), edge_point(cb, hb, ca));
        draw_line(&mut img, start, end, stroke, colour, link.style);
        let head = ARROW_SIZE * scale;
        if link.arrow.to_end() {
            arrowhead(&mut img, end, sub(end, start), head, colour);
        }
        if link.arrow.to_start() {
            arrowhead(&mut img, start, sub(start, end), head, colour);
        }
        let mid = mul(add(start, end), 0.5);
        if link.label.trim().is_empty() {
            fill_square(&mut img, mid, 7.0 * scale, colour);
        } else {
            let size = 11.0 * scale;
            let label = short(&link.label, 40);
            let tw = text_width(&title_font, &label, size);
            let pad = 5.0 * scale;
            let rect = [
                mid[0] - tw / 2.0 - pad,
                mid[1] - size / 2.0 - pad * 0.6,
                mid[0] + tw / 2.0 + pad,
                mid[1] + size / 2.0 + pad * 0.6,
            ];
            fill(&mut img, rect, Rgba([255, 255, 255, 255]));
            draw_box_outline(
                &mut img,
                mid,
                [rect[2] - rect[0], rect[3] - rect[1]].map(|v| v / 2.0),
                1.0 * scale,
                colour,
                LineStyle::Solid,
            );
            centred_text(
                &mut img,
                &title_font,
                &label,
                mid[0],
                mid[1] + size * 0.33,
                size,
                Rgba([40, 40, 40, 255]),
            );
        }
    }

    // then the boxes themselves, on top of the links
    for (n, size, has_source) in &sizes {
        let kind = graph::kind(&n.kind);
        let [r, g, b] = kind.colour();
        let colour = Rgba([r, g, b, 255]);
        let centre = to_px(n.pos);
        let half = mul([size.w / 2.0, size.h / 2.0], scale);
        let rect = [
            centre[0] - half[0],
            centre[1] - half[1],
            centre[0] + half[0],
            centre[1] + half[1],
        ];
        fill(&mut img, rect, Rgba([250, 250, 250, 255]));
        draw_box_outline(&mut img, centre, half, 1.6 * scale, colour, n.border);

        let left = rect[0] + 10.0 * scale;
        let icon_drawn = if let Some(bytes) = n.picture_bytes() {
            if let Ok(pic) = image::load_from_memory(&bytes) {
                let side = ((rect[3] - rect[1] - 10.0 * scale).min(24.0 * scale)).max(1.0);
                let thumb = pic.thumbnail(side as u32, side as u32).to_rgba8();
                let x = left as i64;
                let y = (centre[1] - side / 2.0) as i64;
                image::imageops::overlay(&mut img, &thumb, x, y);
                true
            } else {
                false
            }
        } else {
            false
        };
        if !icon_drawn {
            draw_text(
                &mut img,
                &icon_font,
                kind.icon,
                left,
                centre[1] + ICON_SIZE * scale * 0.35,
                ICON_SIZE * scale,
                colour,
            );
        }

        let text_x = left + 30.0 * scale;
        let title = short(&n.title(), 30);
        let subtitle = short(&n.subtitle(), 36);
        let title_h = TITLE_SIZE * scale * 1.2;
        let sub_h = SUB_SIZE * scale * 1.2;
        let total_h = title_h + if size.has_sub { sub_h } else { 0.0 };
        let mut y = centre[1] - total_h / 2.0 + title_h * 0.78;
        draw_text(
            &mut img,
            &title_font,
            &title,
            text_x,
            y,
            TITLE_SIZE * scale,
            Rgba([25, 25, 25, 255]),
        );
        if size.has_sub {
            y += sub_h;
            draw_text(
                &mut img,
                &sub_font,
                &subtitle,
                text_x,
                y,
                SUB_SIZE * scale,
                Rgba([110, 110, 110, 255]),
            );
        }

        if *has_source {
            let source = short(&format!("Source: {}", n.source.trim()), 60);
            centred_text(
                &mut img,
                &sub_font,
                &source,
                centre[0],
                rect[3] + 14.0 * scale,
                SOURCE_SIZE * scale,
                Rgba([140, 140, 140, 255]),
            );
        }
    }

    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_graph_still_exports() {
        let img = render(&Graph::default()).unwrap();
        assert!(img.width() > 0 && img.height() > 0);
    }

    #[test]
    fn exported_picture_holds_every_box() {
        let mut g = Graph::default();
        let a = g.add("person", [0.0, 0.0]);
        let b = g.add("email", [400.0, 0.0]);
        g.link(a, b);
        g.links[0].label = "owns".into();
        g.node_mut(a).unwrap().source = "Public records".into();
        let img = render(&g).unwrap();
        // wide enough to hold both boxes with the margin on each side
        assert!(img.width() as f32 > 400.0 * SCALE * 0.5);
        assert!(img.height() > 0);
    }

    #[test]
    fn huge_boards_are_capped() {
        let mut g = Graph::default();
        for i in 0..3 {
            g.add("person", [i as f32 * 40_000.0, 0.0]);
        }
        let img = render(&g).unwrap();
        assert!(img.width() <= MAX_SIDE && img.height() <= MAX_SIDE);
    }

    #[test]
    fn edge_points_land_on_the_box() {
        let near = |p: P, x: f32, y: f32| (p[0] - x).abs() < 1e-3 && (p[1] - y).abs() < 1e-3;
        assert!(near(
            edge_point([0.0, 0.0], [50.0, 20.0], [300.0, 0.0]),
            50.0,
            0.0
        ));
        assert!(near(
            edge_point([0.0, 0.0], [50.0, 20.0], [0.0, 300.0]),
            0.0,
            20.0
        ));
    }
}
