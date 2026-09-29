//! Making a pattern tile without showing its joins.
//!
//! A texture photographed or drawn without tiling in mind does not meet itself: its left
//! edge is nothing like its right, so every repeat lays down a hard line. This module
//! measures how bad that is and, when it is bad, synthesises a larger tile that wraps
//! against itself, by Efros and Freeman's image quilting.
//!
//! Everything here is pure — pixels in, pixels out, no image crate, no egui, no file
//! system — so the measurement, the cutting and the synthesis are all testable on
//! hand-made pixels.

/// A decoded image as plain rows of RGBA, the common currency between the editor's
/// texture loading and the synthesis below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    /// Row-major, `width * height` long.
    pub pixels: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn new(width: usize, height: usize, pixels: Vec<[u8; 4]>) -> Option<Self> {
        (width * height == pixels.len() && width > 0 && height > 0)
            .then_some(Self { width, height, pixels })
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [u8; 4] {
        self.pixels[y * self.width + x]
    }
}

/// Difference between two pixels over their colour channels, ignoring alpha.
#[inline]
fn pixel_diff(a: [u8; 4], b: [u8; 4]) -> f32 {
    let d = |i: usize| (a[i] as f32 - b[i] as f32).abs();
    d(0) + d(1) + d(2)
}

/// Mean absolute colour difference between the column at `x` and the column at `other`.
fn column_diff(img: &Rgba, x: usize, other: usize) -> f32 {
    let total: f32 = (0..img.height).map(|y| pixel_diff(img.at(x, y), img.at(other, y))).sum();
    total / (img.height as f32 * 3.0)
}

/// Mean absolute colour difference between the row at `y` and the row at `other`.
fn row_diff(img: &Rgba, y: usize, other: usize) -> f32 {
    let total: f32 = (0..img.width).map(|x| pixel_diff(img.at(x, y), img.at(x, other))).sum();
    total / (img.width as f32 * 3.0)
}

/// How much worse the wrap is than the picture's own texture, across and down.
///
/// The numerator is the difference between opposite edges: what you see at a tile
/// boundary. The denominator is the average difference between neighbouring lines: what
/// the picture does anyway. A ratio near 1 means the join looks like any other part of
/// the image, which is what seamless means. `cave_stone.png` measures 7.4 across and 9.0
/// down, so its joins are nearly an order of magnitude sharper than its own detail.
///
/// A picture with no internal variation at all has no scale to judge against: a flat
/// colour wraps perfectly and scores 1, while anything else is reported as infinitely
/// bad rather than dividing by zero.
pub fn seam_ratio(img: &Rgba) -> (f32, f32) {
    const FLAT: f32 = 1e-6;

    let across = if img.width < 2 {
        0.0
    } else {
        let seam = column_diff(img, 0, img.width - 1);
        let interior: f32 = (0..img.width - 1).map(|x| column_diff(img, x, x + 1)).sum::<f32>()
            / (img.width - 1) as f32;
        ratio(seam, interior, FLAT)
    };

    let down = if img.height < 2 {
        0.0
    } else {
        let seam = row_diff(img, 0, img.height - 1);
        let interior: f32 = (0..img.height - 1).map(|y| row_diff(img, y, y + 1)).sum::<f32>()
            / (img.height - 1) as f32;
        ratio(seam, interior, FLAT)
    };

    (across, down)
}

fn ratio(seam: f32, interior: f32, flat: f32) -> f32 {
    if interior <= flat {
        if seam <= flat {
            1.0
        } else {
            f32::INFINITY
        }
    } else {
        seam / interior
    }
}

/// The sharpest join anywhere in the picture, against the average join.
///
/// [`seam_ratio`] looks only at the wrap, which is not enough to certify a tile: a
/// synthesis can leave the wrap perfectly continuous and still carry a hard line down
/// the middle, and the wrap measurement will happily report success. This walks every
/// boundary between neighbouring lines, the wrap included, and reports the worst one, so
/// a seam is caught wherever it was left.
pub fn worst_join_ratio(img: &Rgba) -> (f32, f32) {
    const FLAT: f32 = 1e-6;

    let across = if img.width < 2 {
        0.0
    } else {
        let joins: Vec<f32> = (0..img.width)
            .map(|x| column_diff(img, x, (x + 1) % img.width))
            .collect();
        worst(&joins, FLAT)
    };

    let down = if img.height < 2 {
        0.0
    } else {
        let joins: Vec<f32> = (0..img.height)
            .map(|y| row_diff(img, y, (y + 1) % img.height))
            .collect();
        worst(&joins, FLAT)
    };

    (across, down)
}

fn worst(joins: &[f32], flat: f32) -> f32 {
    let mean = joins.iter().sum::<f32>() / joins.len() as f32;
    let max = joins.iter().copied().fold(0.0f32, f32::max);
    ratio(max, mean, flat)
}

/// The sharpest short stretch of any join, and how it compares to the average join.
///
/// [`worst_join_ratio`] averages each join over its whole length, which is the right
/// instrument for a seam running edge to edge and the wrong one for a weld a few dozen
/// pixels long: over five hundred lines, thirty bad ones all but vanish. That blindness
/// let two separate hard lines through. This slides a window along every join instead,
/// so a short one is caught.
///
/// `skip_wrap` leaves out the join between the last line and the first. A picture that
/// does not tile fails precisely there, so including it measures the flaw being removed;
/// excluding it gives the picture's own interior standard, which is the fair bar to hold
/// a tile to. A tile is doing well when its sharpest short join is no worse than the
/// sharpest one the source already had inside itself.
pub fn worst_local_join(img: &Rgba, window: usize, skip_wrap: bool) -> (f32, f32) {
    const FLAT: f32 = 1e-6;
    let (w, h) = (img.width, img.height);
    if w < 2 || h < window || window == 0 {
        return (0.0, 0.0);
    }

    let mut total = 0.0;
    let mut worst = 0.0f32;
    for x in 0..w {
        if skip_wrap && x + 1 == w {
            continue;
        }
        let other = (x + 1) % w;
        let line: Vec<f32> = (0..h)
            .map(|y| pixel_diff(img.at(x, y), img.at(other, y)) / 3.0)
            .collect();
        total += line.iter().sum::<f32>();
        let mut running: f32 = line[..window].iter().sum();
        worst = worst.max(running / window as f32);
        for k in window..h {
            running += line[k] - line[k - window];
            worst = worst.max(running / window as f32);
        }
    }

    let counted = if skip_wrap { w - 1 } else { w };
    let mean = total / (counted * h) as f32;
    (worst, ratio(worst, mean, FLAT))
}

/// The window the measurements above use. Short enough that a weld a dozen pixels long
/// still dominates it, long enough not to be set off by one noisy pixel.
///
/// This is a measurement, not a gate. It was briefly used to refuse a tile locally
/// rougher than its source, and that rejected tiles which looked fine, on fixtures whose
/// numbers I could not account for; refusing to synthesise is a worse failure than
/// synthesising imperfectly, so the decision was withdrawn and the measurement kept.
pub const LOCAL_WINDOW: usize = 8;

/// Above this, a join is sharp enough to be worth resynthesising the tile. A genuinely
/// seamless texture sits near 1; the threshold leaves room for one that is merely
/// imperfect without rebuilding something already good.
pub const SEAM_THRESHOLD: f32 = 2.0;

/// Whether either wrap is sharp enough to be worth quilting.
pub fn needs_quilting(img: &Rgba) -> bool {
    let (across, down) = seam_ratio(img);
    across > SEAM_THRESHOLD || down > SEAM_THRESHOLD
}

/// A small deterministic generator. The synthesis must choose among equally good patches,
/// and that choice must come out the same on every machine and every launch, so it
/// cannot use the system generator, the clock, or anything hashed with a random seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Any non-zero state will do; xorshift stalls at zero.
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 { 0 } else { (self.next() % bound as u64) as usize }
    }
}

/// The column, for each row, at which to stop taking the old pixels and start taking the
/// new ones, following the path of least difference down an overlap region.
///
/// `error` is row-major, `width` wide. Cutting along the cheapest path rather than
/// crossfading is what keeps the result crisp: the join follows features already in the
/// picture instead of smearing two of them together.
pub fn min_error_cut(error: &[f32], width: usize, height: usize) -> Vec<usize> {
    if width == 0 || height == 0 || error.len() != width * height {
        return Vec::new();
    }

    // Cheapest path from the top down to each cell.
    let mut cost = error.to_vec();
    for y in 1..height {
        for x in 0..width {
            let lo = x.saturating_sub(1);
            let hi = (x + 1).min(width - 1);
            let best = (lo..=hi)
                .map(|px| cost[(y - 1) * width + px])
                .fold(f32::INFINITY, f32::min);
            cost[y * width + x] += best;
        }
    }

    // Walk back up from the cheapest finish.
    let last = (height - 1) * width;
    let mut x = (0..width).fold(0, |best, c| {
        if cost[last + c] < cost[last + best] { c } else { best }
    });
    let mut path = vec![0usize; height];
    path[height - 1] = x;
    for y in (0..height - 1).rev() {
        let lo = x.saturating_sub(1);
        let hi = (x + 1).min(width - 1);
        x = (lo..=hi).fold(lo, |best, c| {
            if cost[y * width + c] < cost[y * width + best] { c } else { best }
        });
        path[y] = x;
    }
    path
}

/// How the tile is put together. Derived from the source rather than exposed: one good
/// default, not a panel of sliders.
struct Plan {
    patch: usize,
    overlap: usize,
}

impl Plan {
    /// The largest patch the source can supply with room to choose between positions.
    ///
    /// `None` when the source is too small to quilt at all: with only a handful of
    /// possible patches the synthesis would lay the same one down repeatedly, which is
    /// the picture back again with extra steps, so the honest answer is to decline.
    fn for_source(src: &Rgba) -> Option<Self> {
        let limit = src.width.min(src.height);
        let mut patch = limit.min(96);
        while patch >= 12 {
            let overlap = (patch / 6).max(2);
            // At least eight positions along the shorter side, so the choice is real.
            if limit >= patch + 7 {
                return Some(Self { patch, overlap });
            }
            patch -= 1;
        }
        None
    }

    fn step(&self) -> usize {
        self.patch - self.overlap
    }
}

/// Synthesise a `size` by `size` tile from `src` that wraps against itself.
///
/// Two steps, kept apart so each can be checked on its own. First an ordinary quilt onto
/// a canvas one overlap larger than the tile in each direction: patches laid on an
/// overlapping grid, each chosen to agree with what it meets and joined along the
/// cheapest cut through the overlap. Nothing about that step is circular.
///
/// Then the fold. The strip that runs past the tile's right edge is what the picture
/// naturally continues into, so it is cut onto the tile's left edge, and likewise bottom
/// onto top. Wrapping the tile therefore shows the same continuation the canvas already
/// had, and the join is a minimum-error cut like every other join rather than a butt
/// weld. Making the wrap an explicit step is deliberate: it can be measured, and a seam
/// left anywhere inside the tile shows up rather than hiding behind a continuous edge.
///
/// `None` when the synthesis declines: a picture too small to supply a choice of
/// patches, or one the result fails to improve. Saying so rather than handing back the
/// source lets the caller tell a tile from a picture that was left alone, which matters
/// because only a tile is worth caching.
pub fn quilt(src: &Rgba, size: usize, seed: u64) -> Option<Rgba> {
    let plan = Plan::for_source(src)?;
    if size < plan.patch + plan.overlap {
        return None;
    }

    let canvas = size + plan.overlap;
    let mut out = vec![[0u8; 4]; canvas * canvas];
    let mut written = vec![false; canvas * canvas];
    let mut rng = Rng::new(seed);

    // A row of patches at a time. Each strip is assembled on its own, joining only left
    // to right, and then laid onto the canvas along a single cut that runs the whole way
    // across. Nothing is ever joined at a corner where three patches meet, because no
    // such corner exists: a strip meets what is above it along one seam, not along a
    // separate one per patch. Earlier designs cut each patch against both its left and
    // its upper neighbour, and where those two cuts disagreed they preserved a boundary
    // that neither had measured — a hard line, one pixel wide, at the same place in
    // every patch.
    let mut oy = 0;
    while oy < canvas {
        let strip = build_strip(src, &out, &written, canvas, oy, &plan, &mut rng);
        lay_strip(&mut out, &mut written, canvas, oy, &strip, &plan);
        oy += plan.step();
    }

    let tile = fold(&out, canvas, size, plan.overlap);

    // A last check against wishful thinking. Synthesis is not guaranteed to help: a
    // picture whose brightness is a function of position cannot be rearranged into one
    // that tiles, and a tile whose sharpest join is worse than the original's is not
    // worth having. Comparing like with like — the worst join anywhere, wrap included —
    // the source is the better answer whenever the tile fails to beat it.
    let (src_across, src_down) = worst_join_ratio(src);
    let (tile_across, tile_down) = worst_join_ratio(&tile);
    if tile_across.max(tile_down) >= src_across.max(src_down) {
        return None;
    }

    Some(tile)
}

/// One row of patches, `plan.patch` tall and the full width of the canvas.
struct Strip {
    pixels: Vec<[u8; 4]>,
    written: Vec<bool>,
    width: usize,
}

impl Strip {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        self.pixels[y * self.width + x]
    }
    fn has(&self, x: usize, y: usize) -> bool {
        self.written[y * self.width + x]
    }
}

/// Assemble one strip, laying patches left to right and joining each to the one before
/// it along the cheapest cut through their overlap.
///
/// Patches are still *chosen* with the canvas above in mind, so the strip will sit well
/// on what is already there; it is only the *joining* that is left to `lay_strip`.
fn build_strip(
    src: &Rgba,
    canvas_out: &[[u8; 4]],
    canvas_written: &[bool],
    canvas: usize,
    oy: usize,
    plan: &Plan,
    rng: &mut Rng,
) -> Strip {
    let height = plan.patch;
    let mut strip = Strip {
        pixels: vec![[0u8; 4]; height * canvas],
        written: vec![false; height * canvas],
        width: canvas,
    };

    let mut ox = 0;
    while ox < canvas {
        let (sx, sy) =
            choose_patch(src, &strip, canvas_out, canvas_written, canvas, oy, ox, plan, rng);

        // Only the join to the patch on the left, and only when there is one.
        let left_cut = if ox > 0 {
            strip_left_cut(src, &strip, ox, sx, sy, plan)
        } else {
            Vec::new()
        };

        for j in 0..height {
            for i in 0..plan.patch {
                if ox + i >= canvas {
                    continue;
                }
                let o = j * canvas + (ox + i);
                if strip.written[o]
                    && i < plan.overlap
                    && !left_cut.is_empty()
                    && i < left_cut[j]
                {
                    continue; // still the previous patch's side of the cut
                }
                strip.pixels[o] = src.at(sx + i, sy + j);
                strip.written[o] = true;
            }
        }
        ox += plan.step();
    }
    strip
}

/// The source position that agrees best with both the patch to its left in the strip and
/// whatever the canvas already holds above. Ties, and near ties, go to the seeded
/// generator, which is what stops the tile being one patch repeated.
#[allow(clippy::too_many_arguments)]
fn choose_patch(
    src: &Rgba,
    strip: &Strip,
    canvas_out: &[[u8; 4]],
    canvas_written: &[bool],
    canvas: usize,
    oy: usize,
    ox: usize,
    plan: &Plan,
    rng: &mut Rng,
) -> (usize, usize) {
    let max_x = src.width.saturating_sub(plan.patch);
    let max_y = src.height.saturating_sub(plan.patch);
    if max_x == 0 && max_y == 0 {
        return (0, 0);
    }

    // Where the score comes from: the strip's own left band, and the rows of the canvas
    // this strip will land on. Resolved once, since neither depends on the candidate.
    let mut cells: Vec<(usize, usize, [u8; 4])> = Vec::new();
    for j in 0..plan.patch {
        for i in 0..plan.overlap {
            if ox + i >= canvas {
                continue;
            }
            if strip.has(ox + i, j) {
                cells.push((i, j, strip.at(ox + i, j)));
            }
        }
    }
    for j in 0..plan.overlap {
        let cy = oy + j;
        if cy >= canvas {
            continue;
        }
        for i in plan.overlap..plan.patch {
            if ox + i >= canvas {
                continue;
            }
            let o = cy * canvas + (ox + i);
            if canvas_written[o] {
                cells.push((i, j, canvas_out[o]));
            }
        }
    }

    // Scoring every source position is quadratic in the source and dominates the bake.
    // A sample is enough: the best of a few hundred is visually as good, and it is drawn
    // from the seeded generator so the result stays reproducible.
    const SAMPLE: usize = 220;
    let total = (max_x + 1) * (max_y + 1);
    let positions: Vec<(usize, usize)> = if total <= SAMPLE {
        (0..=max_y).flat_map(|sy| (0..=max_x).map(move |sx| (sx, sy))).collect()
    } else {
        (0..SAMPLE).map(|_| (rng.below(max_x + 1), rng.below(max_y + 1))).collect()
    };

    let mut scored: Vec<((usize, usize), f32)> = Vec::with_capacity(positions.len());
    for (sx, sy) in positions {
        if cells.is_empty() {
            scored.push(((sx, sy), 0.0));
            continue;
        }
        let mut error = 0.0;
        for &(i, j, existing) in &cells {
            error += pixel_diff(existing, src.at(sx + i, sy + j));
        }
        scored.push(((sx, sy), error / cells.len() as f32));
    }

    let best = scored.iter().map(|(_, e)| *e).fold(f32::INFINITY, f32::min);
    // Anything within a tenth again of the best is good enough to pick from, so the tile
    // varies instead of locking onto a single favourite patch.
    let cutoff = best * 1.1 + 1.0;
    let candidates: Vec<(usize, usize)> =
        scored.iter().filter(|(_, e)| *e <= cutoff).map(|(p, _)| *p).collect();
    candidates[rng.below(candidates.len())]
}

/// Where, for each row, a patch takes over from the one to its left.
fn strip_left_cut(
    src: &Rgba,
    strip: &Strip,
    ox: usize,
    sx: usize,
    sy: usize,
    plan: &Plan,
) -> Vec<usize> {
    let mut error = Vec::with_capacity(plan.overlap * plan.patch);
    let mut any = false;
    for j in 0..plan.patch {
        for i in 0..plan.overlap {
            // A cell past the canvas still occupies its place in the surface:
            // `min_error_cut` refuses one that is not exactly overlap by patch, and an
            // empty cut would have the caller overwrite the whole band uncut.
            if ox + i >= strip.width || !strip.has(ox + i, j) {
                error.push(0.0);
                continue;
            }
            any = true;
            error.push(pixel_diff(strip.at(ox + i, j), src.at(sx + i, sy + j)));
        }
    }
    if !any {
        return Vec::new();
    }
    min_error_cut(&error, plan.overlap, plan.patch)
}

/// Lay a strip onto the canvas along one cut that runs the whole way across.
///
/// This is the seam between one row of patches and everything above it, and there is
/// exactly one of them per row rather than one per patch. Because the content on either
/// side of it is uniform — the strip below, the canvas above — the cut has a single
/// well-posed question to answer for each column, and no boundary is left that nothing
/// measured.
fn lay_strip(
    out: &mut [[u8; 4]],
    written: &mut [bool],
    canvas: usize,
    oy: usize,
    strip: &Strip,
    plan: &Plan,
) {
    // For each column, the row at which the strip takes over from what is above.
    let cut = if oy > 0 {
        let mut error = Vec::with_capacity(canvas * plan.overlap);
        let mut any = false;
        for x in 0..canvas {
            for r in 0..plan.overlap {
                let cy = oy + r;
                if cy >= canvas || !strip.has(x, r) || !written[cy * canvas + x] {
                    error.push(0.0);
                    continue;
                }
                any = true;
                error.push(pixel_diff(out[cy * canvas + x], strip.at(x, r)));
            }
        }
        if any {
            min_error_cut(&error, plan.overlap, canvas)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    for j in 0..plan.patch {
        let cy = oy + j;
        if cy >= canvas {
            break;
        }
        for x in 0..canvas {
            if !strip.has(x, j) {
                continue;
            }
            let o = cy * canvas + x;
            if written[o] && j < plan.overlap && !cut.is_empty() && j < cut[x] {
                continue; // still the canvas's side of the seam
            }
            out[o] = strip.at(x, j);
            written[o] = true;
        }
    }
}

/// Cut the canvas's overhanging strips onto its opposite edges, and return the tile.
///
/// `overlap` columns run past the tile on the right: that is what the picture does next,
/// so it is exactly what the tile's left edge should continue into when it repeats. The
/// cut decides, row by row, where to stop showing the continuation and resume the tile's
/// own content, which keeps both boundaries of the band honest: at the far left the
/// continuation meets the tile's right edge, and at the far right the tile meets itself.
fn fold(canvas: &[[u8; 4]], canvas_size: usize, size: usize, overlap: usize) -> Rgba {
    // One direction at a time, the second working on the result of the first. Folding
    // both from the raw canvas would have the vertical pass overwrite the corner the
    // horizontal pass just fixed, with content that never went through it.
    let narrowed = fold_columns(canvas, canvas_size, canvas_size, size, overlap);
    let squared = fold_columns(&transpose(&narrowed, size, canvas_size), canvas_size, size, size, overlap);
    let pixels = transpose(&squared, size, size);
    Rgba { width: size, height: size, pixels }
}

/// Cut the strip beyond `keep` onto the first `overlap` columns, returning `keep`
/// columns by `rows` rows.
///
/// The strip is what the picture does after the last column kept, so it is exactly what
/// the first column should continue into when the result repeats. The cut decides, row
/// by row, where to stop showing that continuation and resume the picture's own content,
/// which keeps both ends honest: on the left the continuation meets the last column, and
/// on the right the picture meets itself.
fn fold_columns(
    src: &[[u8; 4]],
    stride: usize,
    rows: usize,
    keep: usize,
    overlap: usize,
) -> Vec<[u8; 4]> {
    let mut out: Vec<[u8; 4]> = (0..rows)
        .flat_map(|y| (0..keep).map(move |x| (x, y)))
        .map(|(x, y)| src[y * stride + x])
        .collect();

    let error: Vec<f32> = (0..rows)
        .flat_map(|y| (0..overlap).map(move |x| (x, y)))
        .map(|(x, y)| pixel_diff(src[y * stride + keep + x], out[y * keep + x]))
        .collect();
    let cut = min_error_cut(&error, overlap, rows);

    for y in 0..rows {
        // At least the first column, always. The whole point is that the first column
        // continues the last, and that is only true if it comes from the strip. Where
        // the cut has no preference — a band whose error is flat across it — the path
        // can otherwise land on zero and quietly leave the join exactly where it was.
        for x in 0..cut[y].max(1) {
            out[y * keep + x] = src[y * stride + keep + x];
        }
    }
    out
}

/// Rows become columns, so one folding routine can serve both directions.
fn transpose(pixels: &[[u8; 4]], width: usize, height: usize) -> Vec<[u8; 4]> {
    (0..width)
        .flat_map(|x| (0..height).map(move |y| (x, y)))
        .map(|(x, y)| pixels[y * width + x])
        .collect()
}

/// How the synthesis behaves. Bumping this invalidates every cached tile, which is what
/// you want when the algorithm changes and old tiles no longer match what it would now
/// produce.
pub const ALGORITHM_VERSION: u32 = 3;

/// The side of a synthesised tile. Roughly four times the area of a small source, which
/// is enough that the repeat stops reading as a grid.
pub const TILE_SIZE: usize = 512;

/// A stable name for a source image's quilted tile.
///
/// It folds in the modification time as well as the path, so editing an image produces a
/// new entry rather than silently reusing the tile built from its previous contents, and
/// the algorithm version, so old tiles are abandoned rather than mixed with new ones.
/// The same inputs always give the same name on every machine, which is what lets a
/// cache survive a restart.
pub fn cache_key(path: &str, modified_secs: u64, size: usize, version: u32) -> String {
    // FNV-1a: small, stable, and specified, unlike the standard hasher whose output is
    // explicitly not guaranteed between builds.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };
    eat(path.as_bytes());
    eat(&modified_secs.to_le_bytes());
    eat(&(size as u64).to_le_bytes());
    eat(&version.to_le_bytes());
    format!("{hash:016x}")
}

/// The seed for synthesising a given tile. Derived from the same name the cache uses, so
/// a tile rebuilt after the cache is cleared comes out identical to the one that was
/// there before.
pub fn seed_from_key(key: &str) -> u64 {
    let mut hash: u64 = 0x9e37_79b9_7f4a_7c15;
    for b in key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic value noise, so the fixtures below are textures rather than flat
    /// fields but do not depend on a random generator.
    /// Deterministic value noise with a real avalanche, so neighbouring coordinates give
    /// unrelated values. A weaker mix leaves the fixtures below nearly flat, which would
    /// make the measurements they are meant to exercise trivially true.
    fn noise(x: usize, y: usize) -> u8 {
        let mut h = (x as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 29;
        (h >> 32) as u8
    }

    fn grey(v: u8) -> [u8; 4] {
        [v, v, v, 255]
    }

    /// A picture that was never meant to tile: local detail with a slow drift across it,
    /// so the left edge is nothing like the right. This is the realistic shape of the
    /// problem — a photograph whose lighting changes across the frame — and detail still
    /// dominates locally, as it does in a real texture.
    fn a_seamy_texture(w: usize, h: usize) -> Rgba {
        let pixels = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let detail = noise(x, y) % 70;
                // Drift both ways, and strongly. With a drift in x alone the picture is
                // pure uncorrelated noise vertically, and a weld between two noise
                // patches is indistinguishable from ordinary neighbouring rows — a
                // horizontal weld could be introduced and nothing here would notice,
                // which is exactly what happened. A hundred each way sits mid-band: at
                // sixty a weld escapes, and eighty through a hundred and fifty all catch
                // both. A couple of pixels in the far corner do clip at 255, which is
                // immaterial at this scale but worth not misdescribing.
                let drift = (x * 100 / w + y * 100 / h) as u8;
                grey(detail.saturating_add(drift))
            })
            .collect();
        Rgba::new(w, h, pixels).expect("valid fixture")
    }

    /// The hardest case there is: a pure gradient, where brightness *is* position, so
    /// two patches from different places can never agree and every join shows. Kept
    /// separate because no synthesis can make this one look like a texture, and holding
    /// it to the same bar as a real picture would only be a bar that lies.
    fn a_pure_gradient(w: usize, h: usize) -> Rgba {
        let pixels = (0..w * h)
            .map(|i| grey(((i % w) * 240 / w) as u8))
            .collect();
        Rgba::new(w, h, pixels).expect("valid fixture")
    }

    /// A real texture that happens to tile: full-range detail, but sampled on a torus so
    /// the last column genuinely neighbours the first. Deliberately not a flat or
    /// near-flat field, which would pass the measurement for the wrong reason.
    fn a_seamless_texture(w: usize, h: usize) -> Rgba {
        let pixels = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                // Averaging a cell with its wrapped neighbours makes opposite edges
                // continuous while leaving the detail between them intact.
                let at = |dx: usize, dy: usize| noise((x + dx) % w, (y + dy) % h) as u32;
                let v = (at(0, 0) + at(1, 0) + at(0, 1) + at(w - 1, 0) + at(0, h - 1)) / 5;
                grey(v as u8)
            })
            .collect();
        Rgba::new(w, h, pixels).expect("valid fixture")
    }

    // ── Measuring the join ───────────────────────────────────────────────────

    #[test]
    fn a_picture_that_was_never_meant_to_tile_is_reported_as_needing_work() {
        let seamy = a_seamy_texture(64, 64);
        let (across, _down) = seam_ratio(&seamy);
        assert!(
            across > SEAM_THRESHOLD,
            "a ramp across the picture makes its edges unlike each other: got {across}"
        );
        assert!(needs_quilting(&seamy));
    }

    /// Mean difference between neighbouring columns: how much the picture varies at all.
    fn detail_of(img: &Rgba) -> f32 {
        (0..img.width - 1).map(|x| column_diff(img, x, x + 1)).sum::<f32>()
            / (img.width - 1) as f32
    }

    #[test]
    fn the_fixtures_are_textures_rather_than_flat_fields() {
        // Guarding the tests below: a near-flat picture passes a seam measurement
        // through the divide-by-zero branch rather than on its merits, so the fixtures
        // must carry real detail to be worth measuring.
        assert!(detail_of(&a_seamy_texture(64, 64)) > 5.0, "the seamy fixture has detail");
        assert!(detail_of(&a_seamless_texture(64, 64)) > 5.0, "and so does the seamless one");
    }

    #[test]
    fn a_picture_that_already_tiles_is_left_alone() {
        let clean = a_seamless_texture(64, 64);
        let (across, down) = seam_ratio(&clean);
        assert!(across <= SEAM_THRESHOLD, "its join looks like the rest of it: got {across}");
        assert!(down <= SEAM_THRESHOLD, "in both directions: got {down}");
        assert!(!needs_quilting(&clean), "so nothing should be resynthesised");
    }

    #[test]
    fn a_flat_colour_wraps_perfectly_and_is_never_quilted() {
        // No internal variation means no scale to judge a join against; a flat field
        // genuinely has no seam, and must not divide by zero into a false positive.
        let flat = Rgba::new(16, 16, vec![grey(120); 256]).expect("valid");
        assert_eq!(seam_ratio(&flat), (1.0, 1.0));
        assert!(!needs_quilting(&flat));
    }

    #[test]
    fn a_single_pixel_has_no_edges_to_compare() {
        let dot = Rgba::new(1, 1, vec![grey(9)]).expect("valid");
        assert_eq!(seam_ratio(&dot), (0.0, 0.0));
        assert!(!needs_quilting(&dot));
    }

    // ── Synthesis ────────────────────────────────────────────────────────────

    #[test]
    fn quilting_turns_a_hard_join_into_one_that_looks_like_the_rest() {
        let seamy = a_seamy_texture(64, 64);
        let (before, _) = seam_ratio(&seamy);

        let tile = quilt(&seamy, 256, 0x5eed).expect("a texture with detail quilts");
        let (across, down) = seam_ratio(&tile);

        assert_eq!((tile.width, tile.height), (256, 256), "the requested tile size");
        assert!(
            across <= SEAM_THRESHOLD && down <= SEAM_THRESHOLD,
            "the wrap is no worse than the picture's own detail: {across} across, {down} down, \
             from {before} before"
        );
        assert!(!needs_quilting(&tile), "so the result would not itself need quilting");

        // The wrap alone certifies nothing. A synthesis can leave the edges continuous
        // and still carry a hard line down the middle, so the sharpest join anywhere in
        // the tile has to be in range too.
        let (worst_across, worst_down) = worst_join_ratio(&tile);
        assert!(
            worst_across <= SEAM_THRESHOLD && worst_down <= SEAM_THRESHOLD,
            "no join anywhere in the tile stands out: {worst_across} across, {worst_down} down"
        );

        // Nor does a full-length measurement: a weld is short, and averaged over five
        // hundred lines it disappears. This is the check whose absence let two separate
        // hard lines through, held to the only bar that needs no tuning — the sharpest
        // short join the source already had inside itself.
        let (source_local, _) = worst_local_join(&seamy, LOCAL_WINDOW, true);
        let (tile_local, _) = worst_local_join(&tile, LOCAL_WINDOW, false);
        assert!(
            tile_local <= source_local * 2.0,
            "the tile's sharpest short join is {tile_local:.1} against the source's own \
             interior at {source_local:.1}, so the synthesis introduced something the \
             picture never had"
        );
    }

    #[test]
    fn a_pure_gradient_comes_back_untouched_because_quilting_cannot_help_it() {
        // Brightness here is position, so a patch lifted from anywhere disagrees with
        // its neighbours in every direction: quilting trades one seam for several. The
        // guard notices that the result is no better than what it started with, and the
        // honest answer is the picture itself.
        //
        // This is the case that earns the guard. Without it the gradient would come back
        // with its horizontal join softened from 32 to 20 and fresh vertical joins worse
        // than the 1.0 it had, which is a worse picture reported as an improvement.
        let gradient = a_pure_gradient(64, 64);
        let (before_across, before_down) = worst_join_ratio(&gradient);
        assert!(before_across > 8.0, "its wrap is its worst join: {before_across}");
        assert_eq!(before_down, 1.0, "and it has no vertical variation at all to spoil");

        assert_eq!(
            quilt(&gradient, 256, 0x5eed),
            None,
            "so the synthesis is refused rather than handing back a worse picture"
        );
    }

    /// Mean colour difference across the join between column `x` and its neighbour,
    /// over a band of rows. Short bands are the point: a weld is local, and averaging a
    /// join over its whole length hides one.
    fn band_col_join(img: &Rgba, x: usize, rows: std::ops::Range<usize>) -> f32 {
        let n = rows.len();
        let other = (x + 1) % img.width;
        rows.map(|y| pixel_diff(img.at(x, y), img.at(other, y))).sum::<f32>() / (n as f32 * 3.0)
    }

    /// The same across the join between row `y` and the one below, over a band of
    /// columns. The seam where one strip meets the next runs this way, and a weld there
    /// would be missed by looking only down the columns.
    fn band_row_join(img: &Rgba, y: usize, cols: std::ops::Range<usize>) -> f32 {
        let n = cols.len();
        let other = (y + 1) % img.height;
        cols.map(|x| pixel_diff(img.at(x, y), img.at(x, other))).sum::<f32>() / (n as f32 * 3.0)
    }

    #[test]
    fn no_join_stands_out_at_the_patch_grid() {
        // The test both earlier failures needed and neither had. A butt weld does not
        // fall just anywhere: it falls where patches meet, at a fixed multiple of the
        // step, and it is short, so averaging a join over its whole length dilutes it
        // away. This measures short bands and asks the one question that separates a
        // defect from ordinary texture: are the joins at the grid worse than the joins
        // everywhere else? Texture does not know where the grid is; a weld only ever
        // happens there.
        let seamy = a_seamy_texture(64, 64);
        let plan = Plan::for_source(&seamy).expect("the fixture quilts");
        let step = plan.step();
        let tile = quilt(&seamy, 256, 0x5eed).expect("quilts");

        for band in [0..16usize, 16..32, tile.height - 16..tile.height] {
            let joins: Vec<f32> =
                (0..tile.width).map(|x| band_col_join(&tile, x, band.clone())).collect();
            let overall = joins.iter().sum::<f32>() / joins.len() as f32;

            // Two different grid positions can carry a seam and both have, historically.
            // A raw edge preserved inside the overlap shows at `overlap - 1` past the
            // patch origin. A butt weld — an overlap written whole because its cut came
            // back empty — shows at `step - 1`, the column immediately before the next
            // origin. Checking only one of them is how a defect of each kind got through
            // a round of this, so check both.
            let at_grid: Vec<f32> = (0..tile.width)
                .filter(|x| x % step == plan.overlap - 1 || x % step == step - 1)
                .map(|x| joins[x])
                .collect();
            assert!(!at_grid.is_empty(), "the fixture must actually have grid joins to check");
            let grid_mean = at_grid.iter().sum::<f32>() / at_grid.len() as f32;
            let worst_grid = at_grid.iter().copied().fold(0.0f32, f32::max);

            assert!(
                grid_mean <= overall * 1.6,
                "rows {band:?}: joins at the patch grid average {grid_mean:.1} against \
                 {overall:.1} everywhere else, which is a seam the grid put there"
            );
            assert!(
                worst_grid <= overall * 2.5,
                "rows {band:?}: the worst join at the grid is {worst_grid:.1} against an \
                 average of {overall:.1}"
            );
        }

        // And the same down the other axis. One strip meets the next along a horizontal
        // seam, and a weld there shows between rows, not between columns: looking only
        // one way would miss half of what can go wrong, which is how an unguarded seam
        // survived a round of this.
        for band in [0..16usize, 16..32, tile.width - 16..tile.width] {
            let joins: Vec<f32> =
                (0..tile.height).map(|y| band_row_join(&tile, y, band.clone())).collect();
            let overall = joins.iter().sum::<f32>() / joins.len() as f32;

            // The same two positions down this axis: the handover inside the strip's
            // overlap, and the row immediately before the next strip begins, which is
            // where a whole overlap written without a cut leaves its edge.
            let at_seam: Vec<f32> = (0..tile.height)
                .filter(|y| y % step == plan.overlap - 1 || y % step == step - 1)
                .map(|y| joins[y])
                .collect();
            assert!(!at_seam.is_empty(), "the fixture must actually have strip seams");
            let seam_mean = at_seam.iter().sum::<f32>() / at_seam.len() as f32;
            let worst_seam = at_seam.iter().copied().fold(0.0f32, f32::max);

            assert!(
                seam_mean <= overall * 1.6,
                "columns {band:?}: rows where one strip meets the next average \
                 {seam_mean:.1} against {overall:.1} elsewhere"
            );
            assert!(
                worst_seam <= overall * 2.5,
                "columns {band:?}: the worst row at a strip seam is {worst_seam:.1} \
                 against an average of {overall:.1}"
            );
        }
    }

    #[test]
    fn a_picture_taller_than_it_is_wide_quilts_without_confusing_its_axes() {
        // Every other fixture here is square, which makes a width-for-height swap
        // anywhere in the synthesis invisible: the two are equal, so the mutation is a
        // no-op on the whole suite. It is not a no-op on a real picture — the user's is
        // 126 by 104 — where it reads past the end of the source and panics on the draw
        // path. Two shapes, neither square, so the axes cannot be confused unnoticed.
        for (w, h) in [(64usize, 40usize), (40, 64)] {
            let src = a_seamy_texture(w, h);
            let tile = quilt(&src, 128, 0x5eed)
                .unwrap_or_else(|| panic!("a {w}x{h} picture should quilt"));

            assert_eq!((tile.width, tile.height), (128, 128), "a square tile from {w}x{h}");
            let (across, down) = seam_ratio(&tile);
            assert!(
                across <= SEAM_THRESHOLD && down <= SEAM_THRESHOLD,
                "and it wraps: {across} across, {down} down, from a {w}x{h} source"
            );
        }
    }

    #[test]
    fn the_synthesis_copies_and_cuts_and_never_invents_a_colour() {
        use std::collections::HashSet;
        let seamy = a_seamy_texture(48, 48);
        let palette: HashSet<[u8; 4]> = seamy.pixels.iter().copied().collect();

        let tile = quilt(&seamy, 128, 0x1234).expect("quilts");

        assert!(
            tile.pixels.iter().all(|p| palette.contains(p)),
            "every pixel of the tile came from the source picture"
        );
    }

    #[test]
    fn the_same_picture_always_gives_the_same_tile() {
        // A level must look identical on every launch and every machine.
        let seamy = a_seamy_texture(48, 48);
        assert_eq!(
            quilt(&seamy, 128, 0xabcd),
            quilt(&seamy, 128, 0xabcd),
            "same source, same seed, same tile"
        );
    }

    #[test]
    fn a_different_seed_lays_the_patches_out_differently() {
        // Proving the determinism above is a seeded choice, not a fixed arrangement.
        let seamy = a_seamy_texture(48, 48);
        assert_ne!(quilt(&seamy, 128, 1), quilt(&seamy, 128, 2));
    }

    #[test]
    fn a_picture_too_small_to_quilt_comes_back_untouched() {
        // There is no room for a patch and an overlap, so the honest answer is the
        // picture itself rather than a crash or an empty tile.
        let tiny = a_seamy_texture(10, 10);
        assert_eq!(quilt(&tiny, 256, 7), None);

        let dot = Rgba::new(1, 1, vec![grey(3)]).expect("valid");
        assert_eq!(quilt(&dot, 256, 7), None);
    }

    #[test]
    fn a_tile_smaller_than_one_patch_comes_back_untouched() {
        let seamy = a_seamy_texture(64, 64);
        assert_eq!(quilt(&seamy, 4, 7), None, "nothing sensible to lay out in four pixels");
    }

    // ── Folding the overhang back onto the start ─────────────────────────────

    /// A canvas whose brightness climbs steadily left to right, so cropping it leaves a
    /// glaring join where the bright right edge meets the dark left one, and the strip
    /// beyond the crop is exactly what should continue past that edge.
    fn a_climbing_canvas(size: usize, overlap: usize) -> Vec<[u8; 4]> {
        let canvas = size + overlap;
        (0..canvas * canvas).map(|i| grey(((i % canvas) * 8) as u8)).collect()
    }

    /// Mean difference between two columns of a raw pixel buffer.
    fn columns_apart(px: &[[u8; 4]], stride: usize, rows: usize, a: usize, b: usize) -> f32 {
        let total: f32 = (0..rows)
            .map(|y| pixel_diff(px[y * stride + a], px[y * stride + b]))
            .sum();
        total / (rows as f32 * 3.0)
    }

    #[test]
    fn wrapping_a_folded_tile_shows_what_the_canvas_showed_next() {
        // The promise of the fold, stated exactly: after it, reading off the tile's
        // right edge and round to its left shows the same step the canvas took from its
        // last kept column into the strip. Whether that step is small is a property of
        // the picture; that the tile inherits it rather than inventing a worse one is
        // the property of the fold, and it holds even for content nothing could tile.
        let (size, overlap) = (16usize, 4usize);
        let canvas_size = size + overlap;
        let canvas = a_climbing_canvas(size, overlap);

        let canvas_step = columns_apart(&canvas, canvas_size, size, size - 1, size);
        let cropped_wrap = columns_apart(&canvas, canvas_size, size, size - 1, 0);

        let folded = fold(&canvas, canvas_size, size, overlap);
        let folded_wrap = columns_apart(&folded.pixels, size, size, size - 1, 0);

        assert!(
            cropped_wrap > canvas_step * 4.0,
            "cropping alone leaves a join far worse than the canvas's own: \
             {cropped_wrap} against {canvas_step}"
        );
        assert!(
            folded_wrap <= canvas_step + 0.001,
            "folding leaves the canvas's own step and no more: {folded_wrap} against {canvas_step}"
        );
        assert_eq!(
            folded.pixels[0], canvas[size],
            "because the tile's first column is the one that followed its last"
        );
        assert_eq!((folded.width, folded.height), (size, size), "the tile, not the canvas");
    }

    #[test]
    fn the_fold_takes_the_strip_even_when_the_cut_has_no_preference() {
        // A band whose error is the same everywhere gives the cut nothing to go on. It
        // must still carry the first column, or the fold would be a no-op exactly when
        // it is needed most.
        let (size, overlap) = (8usize, 3usize);
        let canvas_size = size + overlap;
        let mut canvas = vec![grey(10); canvas_size * canvas_size];
        for y in 0..canvas_size {
            for x in size..canvas_size {
                canvas[y * canvas_size + x] = grey(200); // a uniformly different strip
            }
        }

        let folded = fold(&canvas, canvas_size, size, overlap);

        assert_eq!(
            folded.pixels[0],
            grey(200),
            "the strip came across even with no cheapest path to follow"
        );
    }

    // ── Cutting ──────────────────────────────────────────────────────────────

    /// A strip with everything left of `laid` already laid down, which is what a patch
    /// meets when it is put beside one already placed.
    fn a_strip_filled_to(src: &Rgba, width: usize, height: usize, laid: usize) -> Strip {
        let mut strip = Strip {
            pixels: vec![[0u8; 4]; height * width],
            written: vec![false; height * width],
            width,
        };
        for y in 0..height {
            for x in 0..laid.min(width) {
                strip.pixels[y * width + x] = src.at(x % src.width, y % src.height);
                strip.written[y * width + x] = true;
            }
        }
        strip
    }

    #[test]
    fn a_patch_running_past_the_strip_still_gets_a_full_cut() {
        // A patch at the far end has cells beyond the strip. If those are skipped rather
        // than given a place in the error surface, the surface comes out short,
        // `min_error_cut` refuses it as mismatched and returns nothing, and the caller
        // then overwrites the whole overlap uncut — a butt weld at every patch.
        let plan = Plan { patch: 8, overlap: 3 };
        let src = a_seamy_texture(16, 16);
        let strip = a_strip_filled_to(&src, 20, plan.patch, 19);

        let cut = strip_left_cut(&src, &strip, 16, 0, 0, &plan);

        assert_eq!(cut.len(), plan.patch, "a full-height cut, not an empty one");
        assert!(cut.iter().all(|&c| c < plan.overlap), "and it stays inside the band");
    }

    #[test]
    fn the_first_patch_of_a_strip_has_nothing_to_join_to() {
        let plan = Plan { patch: 8, overlap: 3 };
        let src = a_seamy_texture(16, 16);
        let empty = a_strip_filled_to(&src, 20, plan.patch, 0);

        assert!(
            strip_left_cut(&src, &empty, 0, 0, 0, &plan).is_empty(),
            "nothing laid down yet, so the patch is written whole"
        );
    }

    #[test]
    fn a_strip_meets_what_is_above_it_along_one_seam_not_one_per_patch() {
        // The property the whole design rests on. Within a strip patches join only left
        // to right; the strip as a whole then meets the canvas along a single cut that
        // runs the full width. There is no corner where three patches meet, so there is
        // no boundary left for two disagreeing cuts to preserve between them.
        let plan = Plan { patch: 8, overlap: 3 };
        let canvas = 24;
        let src = a_seamy_texture(16, 16);

        // Everything above filled with one value, the strip with another, so which side
        // a pixel came from is unambiguous.
        let mut out = vec![grey(10); canvas * canvas];
        let mut written = vec![true; canvas * canvas];
        let strip = Strip {
            pixels: vec![grey(200); plan.patch * canvas],
            written: vec![true; plan.patch * canvas],
            width: canvas,
        };

        let oy = 8;
        lay_strip(&mut out, &mut written, canvas, oy, &strip, &plan);

        for x in 0..canvas {
            // Below the overlap the strip always wins.
            for j in plan.overlap..plan.patch {
                assert_eq!(out[(oy + j) * canvas + x], grey(200), "row {j} is the strip's");
            }
            // Inside the overlap every column switches over exactly once, top to bottom:
            // canvas above the cut, strip below it, never alternating.
            let mut seen_strip = false;
            for j in 0..plan.overlap {
                let v = out[(oy + j) * canvas + x];
                if v == grey(200) {
                    seen_strip = true;
                } else {
                    assert!(
                        !seen_strip,
                        "column {x} went back to the canvas after the strip took over"
                    );
                }
            }
        }
    }

    // ── Cutting ──────────────────────────────────────────────────────────────

    #[test]
    fn the_cut_runs_down_the_column_that_matches_best() {
        // One cheap column among expensive ones: the join should follow it.
        let width = 3;
        let height = 4;
        let error: Vec<f32> = (0..height).flat_map(|_| [9.0, 0.0, 9.0]).collect();

        assert_eq!(min_error_cut(&error, width, height), vec![1, 1, 1, 1]);
    }

    #[test]
    fn the_cut_follows_a_diagonal_of_least_difference() {
        // The cheap path moves across as it descends; the cut must move with it rather
        // than holding a straight line through expensive pixels.
        let width = 3;
        let error = vec![
            0.0, 9.0, 9.0,
            9.0, 0.0, 9.0,
            9.0, 9.0, 0.0,
        ];
        assert_eq!(min_error_cut(&error, width, 3), vec![0, 1, 2]);
    }

    #[test]
    fn an_empty_overlap_has_no_cut() {
        assert!(min_error_cut(&[], 0, 0).is_empty());
        assert!(min_error_cut(&[1.0], 2, 3).is_empty(), "a mismatched surface is refused");
    }

    // ── Naming a cached tile ─────────────────────────────────────────────────

    #[test]
    fn the_cache_name_changes_when_anything_it_depends_on_does() {
        let base = cache_key("/tmp/stone.png", 100, TILE_SIZE, ALGORITHM_VERSION);

        assert_eq!(base, cache_key("/tmp/stone.png", 100, TILE_SIZE, ALGORITHM_VERSION), "stable");
        assert_ne!(base, cache_key("/tmp/other.png", 100, TILE_SIZE, ALGORITHM_VERSION), "path");
        assert_ne!(base, cache_key("/tmp/stone.png", 101, TILE_SIZE, ALGORITHM_VERSION), "edited");
        assert_ne!(base, cache_key("/tmp/stone.png", 100, 256, ALGORITHM_VERSION), "tile size");
        assert_ne!(base, cache_key("/tmp/stone.png", 100, TILE_SIZE, 99), "algorithm");
    }

    #[test]
    fn a_rebuilt_tile_matches_the_one_the_cache_held() {
        // Clearing the cache must not change how a level looks, so the seed comes from
        // the same name the cache file had.
        let key = cache_key("/tmp/stone.png", 100, TILE_SIZE, ALGORITHM_VERSION);
        assert_eq!(seed_from_key(&key), seed_from_key(&key));
        assert_ne!(seed_from_key(&key), seed_from_key("a different key"));
    }
}

