//! Depix: recovering text from a pixelated (mosaic-blurred) screenshot. Pixelation replaces every
//! square block of the text by its average colour; when the same font was used to type a
//! reference (a De Bruijn sequence holding every short run of characters), each block can be
//! matched against every same-sized window of the reference, and the windows that match best —
//! judged together with the blocks around them — are pasted back in place of the blocks.

use anyhow::{bail, Result};
use image::{Rgba, RgbaImage};

/// How a pixelated picture is cut into blocks: their side and where the first grid lines fall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub block: u32,
    pub x0: u32,
    pub y0: u32,
}

/// Neighbouring blocks count for this much of the block itself when judging a match.
const NEIGHBOUR_WEIGHT: f32 = 0.5;

fn differs(a: &Rgba<u8>, b: &Rgba<u8>) -> bool {
    (0..3)
        .map(|k| (a[k] as i32 - b[k] as i32).abs())
        .sum::<i32>()
        > 6
}

/// The most common value of `values`, if any.
fn mode(values: impl Iterator<Item = u32>) -> Option<u32> {
    let mut counts = std::collections::HashMap::new();
    for v in values {
        *counts.entry(v).or_insert(0u32) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(v, n)| (n, std::cmp::Reverse(v)))
        .map(|(v, _)| v)
}

/// Finds the block size and grid of the pixelation inside `region` (left, top, right, bottom):
/// colour only changes on grid lines, so the shortest common distance between changes is the
/// block, and the changes' common remainder is where the grid starts.
pub fn detect_grid(img: &RgbaImage, region: [u32; 4]) -> Option<Grid> {
    let [l, t, r, b] = region;
    let (r, b) = (r.min(img.width()), b.min(img.height()));
    if r <= l + 2 || b <= t + 2 {
        return None;
    }
    // x (or y) of every change of colour, row by row and column by column, and the runs between
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut runs: Vec<u32> = Vec::new();
    for y in t..b {
        let mut last = None;
        for x in l + 1..r {
            if differs(img.get_pixel(x, y), img.get_pixel(x - 1, y)) {
                xs.push(x);
                if let Some(last) = last {
                    runs.push(x - last);
                }
                last = Some(x);
            }
        }
    }
    for x in l..r {
        let mut last = None;
        for y in t + 1..b {
            if differs(img.get_pixel(x, y), img.get_pixel(x, y - 1)) {
                ys.push(y);
                if let Some(last) = last {
                    runs.push(y - last);
                }
                last = Some(y);
            }
        }
    }
    if runs.is_empty() {
        return None;
    }
    // the smallest run that is common (a tenth of the changes), not a stray one-pixel edge
    let mut counts = std::collections::BTreeMap::new();
    for run in runs.iter().filter(|r| **r >= 2) {
        *counts.entry(*run).or_insert(0usize) += 1;
    }
    let total: usize = counts.values().sum();
    let block = counts
        .iter()
        .find(|(_, n)| **n * 10 >= total)
        .map(|(run, _)| *run)?;
    let x0 = mode(xs.iter().map(|x| x % block))?;
    let y0 = mode(ys.iter().map(|y| y % block))?;
    Some(Grid { block, x0, y0 })
}

/// Per-channel running sums, so any window's total costs four lookups.
struct Integral {
    w: usize,
    sums: Vec<[u64; 3]>,
}

impl Integral {
    fn new(img: &RgbaImage) -> Self {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut sums = vec![[0u64; 3]; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = [0u64; 3];
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32);
                for k in 0..3 {
                    row[k] += p[k] as u64;
                    sums[(y + 1) * (w + 1) + x + 1][k] = sums[y * (w + 1) + x + 1][k] + row[k];
                }
            }
        }
        Self { w: w + 1, sums }
    }

    /// The average colour of the `size` × `size` window at (`x`, `y`).
    fn average(&self, x: usize, y: usize, size: usize) -> [f32; 3] {
        let at = |x: usize, y: usize| self.sums[y * self.w + x];
        let (a, b, c, d) = (
            at(x, y),
            at(x + size, y),
            at(x, y + size),
            at(x + size, y + size),
        );
        let area = (size * size) as f32;
        std::array::from_fn(|k| (d[k] + a[k] - b[k] - c[k]) as f32 / area)
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum()
}

/// The `top` windows of the reference whose averages, and whose neighbours' averages, best match
/// the block `cell` of `blocks` (a `cols` × `rows` grid of colours).
#[allow(clippy::too_many_arguments)]
fn best_windows(
    blocks: &[[f32; 3]],
    cols: usize,
    rows: usize,
    cell: usize,
    averages: &Integral,
    reference: (usize, usize),
    size: usize,
    top: usize,
) -> Vec<(usize, usize)> {
    let (cx, cy) = (cell % cols, cell / cols);
    let (w, h) = reference;
    // the neighbours that exist, as (blocks away across, down, weight)
    let mut terms: Vec<(isize, isize, f32, [f32; 3])> = vec![(0, 0, 1.0, blocks[cell])];
    for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
        let (nx, ny) = (cx as isize + dx, cy as isize + dy);
        if nx >= 0 && ny >= 0 && (nx as usize) < cols && (ny as usize) < rows {
            terms.push((
                dx,
                dy,
                NEIGHBOUR_WEIGHT,
                blocks[ny as usize * cols + nx as usize],
            ));
        }
    }
    let mut best: Vec<(f32, usize, usize)> = Vec::with_capacity(top + 1);
    for y in 0..=h - size {
        for x in 0..=w - size {
            let (mut score, mut weight) = (0.0f32, 0.0f32);
            for &(dx, dy, wt, colour) in &terms {
                let (wx, wy) = (
                    x as isize + dx * size as isize,
                    y as isize + dy * size as isize,
                );
                if wx < 0 || wy < 0 || wx as usize + size > w || wy as usize + size > h {
                    continue;
                }
                score += wt * distance(colour, averages.average(wx as usize, wy as usize, size));
                weight += wt;
                // already worse than the worst kept match: stop early
                if best.len() == top && score / weight.max(1.0) > best[top - 1].0 * 4.0 {
                    break;
                }
            }
            if weight == 0.0 {
                continue;
            }
            let score = score / weight;
            if best.len() < top || score < best[best.len() - 1].0 {
                let at = best.partition_point(|b| b.0 <= score);
                best.insert(at, (score, x, y));
                best.truncate(top);
            }
        }
    }
    best.into_iter().map(|(_, x, y)| (x, y)).collect()
}

/// Rebuilds the text under `region` (left, top, right, bottom, in `target`'s pixels) of a
/// pixelated picture cut into `grid`'s blocks, from `reference`, a picture of the same font's
/// text. `top` is how many of the best windows are averaged for each block.
pub fn recover(
    target: &RgbaImage,
    region: [u32; 4],
    grid: Grid,
    reference: &RgbaImage,
    top: usize,
) -> Result<RgbaImage> {
    let size = grid.block as usize;
    if size < 2 {
        bail!("the blocks must be at least 2 pixels wide");
    }
    let [l, t, r, b] = region;
    let (r, b) = (r.min(target.width()), b.min(target.height()));
    // the blocks lying wholly inside the region
    let first = |from: u32, origin: u32| {
        let m = grid.block;
        let rem = (from + m - origin % m) % m;
        if rem == 0 {
            from
        } else {
            from + m - rem
        }
    };
    let (bx, by) = (first(l, grid.x0), first(t, grid.y0));
    if r <= bx || b <= by {
        bail!("select the pixelated text first");
    }
    let (cols, rows) = (
        ((r - bx) / grid.block) as usize,
        ((b - by) / grid.block) as usize,
    );
    if cols == 0 || rows == 0 {
        bail!("the selection holds no whole block");
    }
    if reference.width() < 3 * grid.block || reference.height() < 3 * grid.block {
        bail!("the reference picture is smaller than a few blocks");
    }
    let blocks: Vec<[f32; 3]> = (0..cols * rows)
        .map(|i| {
            let x = bx + (i % cols) as u32 * grid.block + grid.block / 2;
            let y = by + (i / cols) as u32 * grid.block + grid.block / 2;
            let p = target.get_pixel(x, y);
            [p[0] as f32, p[1] as f32, p[2] as f32]
        })
        .collect();
    let averages = Integral::new(reference);
    let dims = (reference.width() as usize, reference.height() as usize);
    let top = top.max(1);

    // blocks are independent: share them out over the cores
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let per = (blocks.len() / threads).max(1);
    let mut matches: Vec<Vec<(usize, usize)>> = vec![Vec::new(); blocks.len()];
    std::thread::scope(|scope| {
        let (blocks, averages) = (&blocks, &averages);
        let workers: Vec<_> = matches
            .chunks_mut(per)
            .enumerate()
            .map(|(n, chunk)| {
                scope.spawn(move || {
                    for (k, slot) in chunk.iter_mut().enumerate() {
                        *slot = best_windows(
                            blocks,
                            cols,
                            rows,
                            n * per + k,
                            averages,
                            dims,
                            size,
                            top,
                        );
                    }
                })
            })
            .collect();
        for w in workers {
            let _ = w.join();
        }
    });

    let mut out = RgbaImage::from_pixel(
        cols as u32 * grid.block,
        rows as u32 * grid.block,
        Rgba([255, 255, 255, 255]),
    );
    for (i, found) in matches.iter().enumerate() {
        if found.is_empty() {
            continue;
        }
        let (ox, oy) = (
            (i % cols) as u32 * grid.block,
            (i / cols) as u32 * grid.block,
        );
        for dy in 0..grid.block {
            for dx in 0..grid.block {
                let mut sum = [0u32; 3];
                for &(x, y) in found {
                    let p = reference.get_pixel(x as u32 + dx, y as u32 + dy);
                    for k in 0..3 {
                        sum[k] += p[k] as u32;
                    }
                }
                let n = found.len() as u32;
                out.put_pixel(
                    ox + dx,
                    oy + dy,
                    Rgba([
                        (sum[0] / n) as u8,
                        (sum[1] / n) as u8,
                        (sum[2] / n) as u8,
                        255,
                    ]),
                );
            }
        }
    }
    Ok(out)
}

/// The De Bruijn sequence B(k, n) over `alphabet`: a text holding every run of `n` of its
/// characters exactly once, so typing it in the target's font gives a window for every
/// short word.
pub fn de_bruijn(alphabet: &[char], n: usize) -> String {
    let k = alphabet.len();
    if k == 0 || n == 0 {
        return String::new();
    }
    let mut a = vec![0usize; k * n];
    let mut sequence = Vec::new();
    fn db(t: usize, p: usize, n: usize, k: usize, a: &mut [usize], out: &mut Vec<usize>) {
        if t > n {
            if n % p == 0 {
                out.extend_from_slice(&a[1..=p]);
            }
        } else {
            a[t] = a[t - p];
            db(t + 1, p, n, k, a, out);
            for j in a[t - p] + 1..k {
                a[t] = j;
                db(t + 1, t, n, k, a, out);
            }
        }
    }
    db(1, 1, n, k, &mut a, &mut sequence);
    sequence.into_iter().map(|i| alphabet[i]).collect()
}

/// `text` cut into lines of `width` characters.
pub fn wrapped(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(width.max(1))
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::FontRef;

    /// Black `text` on white, in the font Ridge ships.
    fn render(text: &str, width: u32, height: u32, x: f32) -> RgbaImage {
        let font = FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR).unwrap();
        let mut img = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        crate::annotate::draw_text(&mut img, &font, text, x, 22.0, 20.0, Rgba([0, 0, 0, 255]));
        img
    }

    fn pixelate(img: &RgbaImage, block: u32, ox: u32, oy: u32) -> RgbaImage {
        let mut out = img.clone();
        let mut y = oy;
        while y + block <= img.height() {
            let mut x = ox;
            while x + block <= img.width() {
                let mut sum = [0u32; 3];
                for dy in 0..block {
                    for dx in 0..block {
                        let p = img.get_pixel(x + dx, y + dy);
                        for k in 0..3 {
                            sum[k] += p[k] as u32;
                        }
                    }
                }
                let n = block * block;
                for dy in 0..block {
                    for dx in 0..block {
                        out.put_pixel(
                            x + dx,
                            y + dy,
                            Rgba([
                                (sum[0] / n) as u8,
                                (sum[1] / n) as u8,
                                (sum[2] / n) as u8,
                                255,
                            ]),
                        );
                    }
                }
                x += block;
            }
            y += block;
        }
        out
    }

    #[test]
    fn de_bruijn_holds_every_run_once() {
        let alphabet: Vec<char> = "abc".chars().collect();
        let s = de_bruijn(&alphabet, 3);
        assert_eq!(s.len(), 27);
        let cyclic: Vec<char> = s.chars().chain(s.chars().take(2)).collect();
        let mut runs: Vec<String> = cyclic.windows(3).map(|w| w.iter().collect()).collect();
        runs.sort();
        runs.dedup();
        assert_eq!(runs.len(), 27);
    }

    #[test]
    fn wrapping_cuts_lines() {
        assert_eq!(wrapped("abcdefg", 3), "abc\ndef\ng");
    }

    #[test]
    fn the_grid_is_found() {
        let text = render("cafe42 beef", 200, 40, 3.0);
        let pixelated = pixelate(&text, 6, 2, 4);
        let grid = detect_grid(&pixelated, [8, 6, 150, 34]).unwrap();
        assert_eq!((grid.block, grid.x0 % 6, grid.y0 % 6), (6, 2, 4));
    }

    #[test]
    fn text_comes_back_closer_than_a_blank_guess() {
        let sequence = de_bruijn(&"0123456789abcdef".chars().collect::<Vec<_>>(), 2);
        let reference = render(&sequence, 2300, 40, 2.0);
        // the secret is typed at an offset the reference never had
        let secret = render("ab3f90", 150, 40, 5.0);
        let pixelated = pixelate(&secret, 6, 0, 4);
        let grid = Grid {
            block: 6,
            x0: 0,
            y0: 4,
        };
        let region = [0, 4, 48, 34];
        let got = recover(&pixelated, region, grid, &reference, 1).unwrap();
        assert_eq!(got.dimensions(), (48, 30));
        let error = |a: &RgbaImage| {
            let mut e = 0u64;
            for y in 0..30 {
                for x in 0..48 {
                    e += (a.get_pixel(x, y)[0] as i64 - secret.get_pixel(x, y + 4)[0] as i64)
                        .unsigned_abs();
                }
            }
            e
        };
        let blank = RgbaImage::from_pixel(48, 30, Rgba([255, 255, 255, 255]));
        assert!(
            error(&got) < error(&blank) * 3 / 4,
            "{} vs {}",
            error(&got),
            error(&blank)
        );
    }
}
