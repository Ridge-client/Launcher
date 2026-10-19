//! Annotated intelligence images: a picture with numbered, coloured open squares on it and a
//! caption for each, exported as one PNG with a legend underneath.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// The colours to mark with, in palette order.
pub const COLOURS: [[u8; 3]; 8] = [
    [0xe5, 0x39, 0x35], // red
    [0xfb, 0x8c, 0x00], // orange
    [0xfd, 0xd8, 0x35], // yellow
    [0x43, 0xa0, 0x47], // green
    [0x00, 0xac, 0xc1], // cyan
    [0x1e, 0x88, 0xe5], // blue
    [0x8e, 0x24, 0xaa], // purple
    [0xff, 0xff, 0xff], // white
];

/// One open square on the picture; its number is its place in the list, from 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mark {
    /// Corners in picture pixels: left, top, right, bottom.
    pub rect: [f32; 4],
    pub colour: [u8; 3],
    pub caption: String,
}

impl Default for Mark {
    fn default() -> Self {
        Self {
            rect: [0.0; 4],
            colour: COLOURS[0],
            caption: String::new(),
        }
    }
}

impl Mark {
    /// The same square with left < right and top < bottom, kept inside a `w` × `h` picture.
    pub fn normalised(rect: [f32; 4], w: f32, h: f32) -> [f32; 4] {
        let (l, r) = (rect[0].min(rect[2]), rect[0].max(rect[2]));
        let (t, b) = (rect[1].min(rect[3]), rect[1].max(rect[3]));
        [
            l.clamp(0.0, w),
            t.clamp(0.0, h),
            r.clamp(0.0, w),
            b.clamp(0.0, h),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Annotated {
    pub name: String,
    /// Ridge's copy of the picture.
    pub image: String,
    pub marks: Vec<Mark>,
}

/// Width of a square's outline and size of its number, for a picture `w` wide.
pub fn stroke_and_badge(w: u32, h: u32) -> (f32, f32) {
    let side = w.max(h) as f32;
    (
        (side / 400.0).clamp(2.0, 12.0),
        (side / 40.0).clamp(18.0, 96.0),
    )
}

/// The picture with its squares, numbers and a legend of the captions underneath.
pub fn render(picture: &RgbaImage, marks: &[Mark]) -> Result<RgbaImage> {
    let number_font =
        FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR).context("number font")?;
    let text_font =
        FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).context("caption font")?;
    let (w, h) = picture.dimensions();
    let (stroke, badge) = stroke_and_badge(w, h);

    // the legend: one line per square with a caption, or its number alone
    let line_h = (badge * 1.15).round();
    let pad = (badge * 0.6).round();
    let legend_h = if marks.is_empty() {
        0
    } else {
        (pad * 2.0 + line_h * marks.len() as f32) as u32
    };
    let mut out = RgbaImage::from_pixel(w, h + legend_h, Rgba([24, 24, 24, 255]));
    image::imageops::overlay(&mut out, picture, 0, 0);

    for (i, mark) in marks.iter().enumerate() {
        let colour = Rgba([mark.colour[0], mark.colour[1], mark.colour[2], 255]);
        let [l, t, r, b] = Mark::normalised(mark.rect, w as f32, h as f32);
        outline(&mut out, [l, t, r, b], stroke, colour);
        // the number on a filled tab at the top-left corner, above the square when there is room
        let label = (i + 1).to_string();
        let text_w = text_width(&number_font, &label, badge * 0.8);
        let tab = [badge.max(text_w + badge * 0.5), badge];
        let top = if t - tab[1] >= 0.0 { t - tab[1] } else { t };
        fill(&mut out, [l, top, l + tab[0], top + tab[1]], colour);
        let ink = ink_on(mark.colour);
        draw_text(
            &mut out,
            &number_font,
            &label,
            l + (tab[0] - text_w) / 2.0,
            top + tab[1] * 0.82,
            badge * 0.8,
            ink,
        );

        // its legend line
        let y = h as f32 + pad + line_h * i as f32;
        fill(&mut out, [pad, y, pad + badge, y + badge], colour);
        let n_w = text_width(&number_font, &label, badge * 0.8);
        draw_text(
            &mut out,
            &number_font,
            &label,
            pad + (badge - n_w) / 2.0,
            y + badge * 0.82,
            badge * 0.8,
            ink,
        );
        let caption = mark.caption.lines().next().unwrap_or_default();
        draw_text(
            &mut out,
            &text_font,
            caption,
            pad + badge * 1.5,
            y + badge * 0.8,
            badge * 0.75,
            Rgba([235, 235, 235, 255]),
        );
    }
    Ok(out)
}

/// Black on light colours, white on dark ones.
fn ink_on(rgb: [u8; 3]) -> Rgba<u8> {
    let luma = 0.299 * rgb[0] as f32 + 0.587 * rgb[1] as f32 + 0.114 * rgb[2] as f32;
    if luma > 150.0 {
        Rgba([0, 0, 0, 255])
    } else {
        Rgba([255, 255, 255, 255])
    }
}

fn fill(img: &mut RgbaImage, [l, t, r, b]: [f32; 4], colour: Rgba<u8>) {
    let (w, h) = img.dimensions();
    let x0 = (l.max(0.0) as u32).min(w);
    let x1 = (r.max(0.0).ceil() as u32).min(w);
    let y0 = (t.max(0.0) as u32).min(h);
    let y1 = (b.max(0.0).ceil() as u32).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            img.put_pixel(x, y, colour);
        }
    }
}

/// An open square: four bands `stroke` wide, inside the rectangle.
fn outline(img: &mut RgbaImage, [l, t, r, b]: [f32; 4], stroke: f32, colour: Rgba<u8>) {
    fill(img, [l, t, r, t + stroke], colour);
    fill(img, [l, b - stroke, r, b], colour);
    fill(img, [l, t, l + stroke, b], colour);
    fill(img, [r - stroke, t, r, b], colour);
}

fn text_width(font: &FontRef, text: &str, size: f32) -> f32 {
    let font = font.as_scaled(PxScale::from(size));
    text.chars().map(|c| font.h_advance(font.glyph_id(c))).sum()
}

/// Draws `text` with its baseline at `y`, blending its edges into the picture.
fn draw_text(
    img: &mut RgbaImage,
    font: &FontRef,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    colour: Rgba<u8>,
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let (w, h) = img.dimensions();
    let mut pen = x;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(pen, y));
        pen += scaled.h_advance(id);
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                return;
            }
            let under = img.get_pixel_mut(px as u32, py as u32);
            for k in 0..3 {
                under[k] = (colour[k] as f32 * coverage + under[k] as f32 * (1.0 - coverage)) as u8;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_are_normalised_and_kept_inside() {
        assert_eq!(
            Mark::normalised([50.0, 80.0, 10.0, -5.0], 40.0, 60.0),
            [10.0, 0.0, 40.0, 60.0]
        );
    }

    #[test]
    fn export_draws_squares_numbers_and_legend() {
        let picture = RgbaImage::from_pixel(400, 300, Rgba([0, 0, 0, 255]));
        let marks = vec![
            Mark {
                rect: [100.0, 100.0, 200.0, 180.0],
                colour: COLOURS[0],
                caption: "Parked car".into(),
            },
            Mark {
                rect: [250.0, 40.0, 350.0, 120.0],
                colour: COLOURS[4],
                caption: String::new(),
            },
        ];
        let out = render(&picture, &marks).unwrap();
        // a legend strip was added under the picture
        assert!(out.height() > 300);
        assert_eq!(out.width(), 400);
        // the first square's outline is red, its inside untouched
        assert_eq!(out.get_pixel(100, 140).0, [0xe5, 0x39, 0x35, 255]);
        assert_eq!(out.get_pixel(150, 140).0, [0, 0, 0, 255]);
        // its number tab sits above it, and something light (the "1") is drawn in it
        let tab: Vec<[u8; 4]> = (100..118)
            .flat_map(|x| (82..100).map(move |y| (x, y)))
            .map(|(x, y)| out.get_pixel(x, y).0)
            .collect();
        assert!(tab.contains(&[0xe5, 0x39, 0x35, 255]));
        assert!(tab.iter().any(|p| p[0] > 0xf0 && p[1] > 0xc0));
        // the legend holds light caption text
        let legend_light = (0..400)
            .flat_map(|x| (300..out.height()).map(move |y| (x, y)))
            .filter(|&(x, y)| out.get_pixel(x, y).0[0] > 200)
            .count();
        assert!(legend_light > 50);
    }

    #[test]
    fn nothing_marked_adds_no_legend() {
        let picture = RgbaImage::from_pixel(50, 40, Rgba([9, 9, 9, 255]));
        let out = render(&picture, &[]).unwrap();
        assert_eq!(out.dimensions(), (50, 40));
    }

    #[test]
    fn ron_round_trip() {
        let a = vec![Annotated {
            name: "Compound".into(),
            image: "C:/x.png".into(),
            marks: vec![Mark {
                rect: [1.0, 2.0, 3.0, 4.0],
                colour: COLOURS[2],
                caption: "Gate".into(),
            }],
        }];
        let back: Vec<Annotated> = ron::from_str(&ron::ser::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
}
