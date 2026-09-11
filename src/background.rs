use std::collections::HashMap;
use std::ops::RangeInclusive;

use eframe::egui;
use egui::{Color32, Pos2, Rect, Vec2};
use tracing::{error, trace};

/// Longest edge of the downscaled minimap texture (pixels).
const MINIMAP_MAX_EDGE: u32 = 256;

/// Maximum tile edge in pixels on either axis (stays under common GPU texture limits).
pub const MAX_TILE_EDGE: u32 = 8192;

/// Two-dimensional tile grid derived from the background image dimensions.
///
/// Tiles are addressed by `(col, row)`; each tile is at most `MAX_TILE_EDGE`
/// pixels wide and tall, and the right/bottom edge tiles are cropped to the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    pub image_w: u32,
    pub image_h: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub cols: u32,
    pub rows: u32,
}

impl TileGrid {
    pub fn new(image_w: u32, image_h: u32) -> Self {
        let tile_w = image_w.min(MAX_TILE_EDGE).max(1);
        let tile_h = image_h.min(MAX_TILE_EDGE).max(1);
        Self {
            image_w,
            image_h,
            tile_w,
            tile_h,
            cols: image_w.div_ceil(tile_w),
            rows: image_h.div_ceil(tile_h),
        }
    }

    /// Actual pixel size of tile `(col, row)`; edge tiles are cropped to the image.
    pub fn tile_size(&self, col: u32, row: u32) -> (u32, u32) {
        let w = self.tile_w.min(self.image_w.saturating_sub(col * self.tile_w));
        let h = self.tile_h.min(self.image_h.saturating_sub(row * self.tile_h));
        (w, h)
    }

    /// World-space top-left corner of tile `(col, row)`.
    pub fn tile_origin(&self, col: u32, row: u32) -> Pos2 {
        Pos2::new((col * self.tile_w) as f32, (row * self.tile_h) as f32)
    }

    /// Inclusive `(cols, rows)` ranges of tiles intersecting `world_rect`, plus a
    /// one-tile margin on every side, clipped to the grid. `None` when nothing is
    /// within reach — e.g. the viewport is entirely beyond the image edge.
    pub fn visible_range(&self, world_rect: Rect) -> Option<(RangeInclusive<u32>, RangeInclusive<u32>)> {
        if self.cols == 0 || self.rows == 0 || world_rect.width() < 0.0 || world_rect.height() < 0.0 {
            return None;
        }
        let axis = |min: f32, max: f32, tile: u32, count: u32| -> Option<RangeInclusive<u32>> {
            let tile = tile as f32;
            let first = ((min.max(0.0) / tile).floor() as i64 - 1).max(0);
            let last = ((max.max(0.0) / tile).floor() as i64 + 1).min(count as i64 - 1);
            if first > last {
                None
            } else {
                Some(first as u32..=last as u32)
            }
        };
        let cols = axis(world_rect.min.x, world_rect.max.x, self.tile_w, self.cols)?;
        let rows = axis(world_rect.min.y, world_rect.max.y, self.tile_h, self.rows)?;
        Some((cols, rows))
    }
}

pub struct BackgroundImageController {
    // Full image stored in memory
    full_image: Option<image::DynamicImage>,
    // Actual image dimensions
    image_width: u32,
    image_height: u32,
    /// Tile grid for the loaded image.
    grid: Option<TileGrid>,
    /// Resident tile textures keyed by `(col, row)`.
    tiles: HashMap<(u32, u32), egui::TextureHandle>,
    /// Full-level thumbnail for the minimap; rebuilt when the image loads.
    minimap_texture: Option<egui::TextureHandle>,
}

impl BackgroundImageController {
    pub fn new() -> Self {
        Self {
            full_image: None,
            image_width: 0,
            image_height: 0,
            grid: None,
            tiles: HashMap::new(),
            minimap_texture: None,
        }
    }

    pub fn load_image(&mut self, path: &std::path::PathBuf) -> Result<(u32, u32), String> {
        trace!("background_load_image_start path={:?}", path);
        match image::open(path) {
            Ok(img) => {
                let width = img.width();
                let height = img.height();
                self.image_width = width;
                self.image_height = height;
                self.full_image = Some(img);
                let grid = TileGrid::new(width, height);
                trace!(
                    "background_tile_grid tile_w={} tile_h={} cols={} rows={}",
                    grid.tile_w, grid.tile_h, grid.cols, grid.rows
                );
                self.grid = Some(grid);
                // Clear textures when loading new image
                self.tiles.clear();
                self.minimap_texture = None;
                trace!("background_load_image_success width={} height={}", width, height);
                Ok((width, height))
            }
            Err(e) => {
                error!("background_load_image_error error={}", e);
                Err(format!("Failed to load image: {}", e))
            }
        }
    }

    /// Draw the background at 1:1 pixels, shifted by `-offset` on both axes.
    ///
    /// `offset` is the shared per-frame integer pixel offset and `viewport` is the
    /// on-screen panel rectangle; only tiles within one tile of the viewport are
    /// resident. Nothing is drawn beyond the image's right or bottom edge.
    pub fn draw(&mut self, ctx: &egui::Context, offset: Vec2, viewport: Rect, painter: &egui::Painter) {
        if self.full_image.is_none() {
            trace!("background_draw_skip full_image_missing");
            return;
        }

        // Safety checks
        if self.image_width == 0 || self.image_height == 0 {
            error!("background_draw_error invalid_dimensions width={} height={}", self.image_width, self.image_height);
            return;
        }

        let Some(grid) = self.grid else {
            error!("background_draw_error grid_missing");
            return;
        };

        // A world point `w` appears at screen `w - offset`, so the visible world
        // rectangle is the viewport shifted by `offset`.
        let world_rect = viewport.translate(offset);
        let Some((cols, rows)) = grid.visible_range(world_rect) else {
            if !self.tiles.is_empty() {
                trace!("background_tiles_released reason=viewport_beyond_image");
                self.tiles.clear();
            }
            return;
        };

        // Release tiles that scrolled out of the window.
        let before = self.tiles.len();
        self.tiles.retain(|(c, r), _| cols.contains(c) && rows.contains(r));
        if self.tiles.len() != before {
            trace!("background_tiles_released count={}", before - self.tiles.len());
        }

        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for row in rows.clone() {
            for col in cols.clone() {
                if !self.tiles.contains_key(&(col, row)) {
                    if let Some(tex) = self.load_tile(ctx, col, row, &grid) {
                        self.tiles.insert((col, row), tex);
                    }
                }
                if let Some(tex) = self.tiles.get(&(col, row)) {
                    let (w, h) = grid.tile_size(col, row);
                    if w == 0 || h == 0 {
                        continue;
                    }
                    let origin = grid.tile_origin(col, row);
                    let dest = Pos2::new(origin.x - offset.x, origin.y - offset.y);
                    let rect = Rect::from_min_size(dest, egui::vec2(w as f32, h as f32));
                    painter.image(tex.id(), rect, uv, Color32::WHITE);
                }
            }
        }
    }

    /// Returns the cached minimap texture if a background image is loaded.
    pub fn minimap_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.full_image.is_none() || self.image_width == 0 || self.image_height == 0 {
            return None;
        }
        if self.minimap_texture.is_none() {
            self.build_minimap_texture(ctx);
        }
        self.minimap_texture.clone()
    }

    fn build_minimap_texture(&mut self, ctx: &egui::Context) {
        let Some(full) = self.full_image.as_ref() else {
            return;
        };
        let w = self.image_width;
        let h = self.image_height;
        let (nw, nh) = if w >= h {
            let nw = MINIMAP_MAX_EDGE.min(w).max(1);
            let nh = ((h as f64) * (nw as f64) / (w as f64)).round().clamp(1.0, f64::from(u32::MAX)) as u32;
            (nw, nh)
        } else {
            let nh = MINIMAP_MAX_EDGE.min(h).max(1);
            let nw = ((w as f64) * (nh as f64) / (h as f64)).round().clamp(1.0, f64::from(u32::MAX)) as u32;
            (nw.max(1), nh)
        };

        let resized = image::imageops::resize(full, nw, nh, image::imageops::FilterType::Triangle);
        let rgba = image::DynamicImage::ImageRgba8(resized).to_rgba8();
        let pixels: Vec<Color32> = rgba
            .as_raw()
            .chunks(4)
            .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
            .collect();

        let image_data = egui::ImageData::Color(
            egui::ColorImage {
                size: [nw as usize, nh as usize],
                pixels,
            }
            .into(),
        );
        let tex = ctx.load_texture("level_minimap", image_data, egui::TextureOptions::default());
        trace!("background_minimap_built nw={} nh={}", nw, nh);
        self.minimap_texture = Some(tex);
    }

    fn load_tile(&self, ctx: &egui::Context, col: u32, row: u32, grid: &TileGrid) -> Option<egui::TextureHandle> {
        let full_img = self.full_image.as_ref()?;

        // Extra safety checks
        if full_img.width() == 0 || full_img.height() == 0 {
            error!("background_load_tile_error empty_image");
            return None;
        }

        let origin = grid.tile_origin(col, row);
        let (x, y) = (origin.x as u32, origin.y as u32);
        let (crop_width, crop_height) = grid.tile_size(col, row);

        if crop_width == 0 || crop_height == 0 {
            trace!("background_load_tile_skip col={} row={} crop_width={} crop_height={}", col, row, crop_width, crop_height);
            return None;
        }

        // Catch any panics during image operations
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cropped = image::imageops::crop_imm(full_img, x, y, crop_width, crop_height);
            let cropped_dynamic = image::DynamicImage::ImageRgba8(cropped.to_image());
            let rgba = cropped_dynamic.to_rgba8();
            let pixels: Vec<Color32> = rgba.as_raw().chunks(4)
                .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                .collect();

            let image_data = egui::ImageData::Color(egui::ColorImage {
                size: [crop_width as usize, crop_height as usize],
                pixels,
            }.into());

            ctx.load_texture(&format!("bg_tile_{}_{}", col, row), image_data, egui::TextureOptions::default())
        }));

        match result {
            Ok(tex) => {
                trace!("background_load_tile_success col={} row={} width={} height={}", col, row, crop_width, crop_height);
                Some(tex)
            }
            Err(e) => {
                error!("background_load_tile_panic col={} row={} width={} height={} error={:?}", col, row, crop_width, crop_height, e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_never_exceed_max_edge_on_either_axis() {
        for (w, h) in [(25600, 720), (1000, 20000), (100_000, 100_000), (8192, 8192), (8193, 8193), (1, 1)] {
            let g = TileGrid::new(w, h);
            assert!(g.tile_w <= MAX_TILE_EDGE && g.tile_h <= MAX_TILE_EDGE, "{w}x{h}: {g:?}");
            for row in 0..g.rows {
                for col in 0..g.cols {
                    let (tw, th) = g.tile_size(col, row);
                    assert!(tw >= 1 && th >= 1 && tw <= MAX_TILE_EDGE && th <= MAX_TILE_EDGE, "{w}x{h} ({col},{row}): {tw}x{th}");
                }
            }
        }
    }

    #[test]
    fn bundled_level_image_is_wide_and_single_row() {
        let g = TileGrid::new(25600, 720);
        assert_eq!((g.tile_w, g.tile_h), (8192, 720));
        assert_eq!((g.cols, g.rows), (4, 1));
        // Last column is cropped to the image edge: 25600 - 3*8192 = 1024.
        assert_eq!(g.tile_size(3, 0), (1024, 720));
    }

    #[test]
    fn tall_image_splits_into_multiple_rows() {
        let g = TileGrid::new(1000, 20000);
        assert!(g.rows > 1, "an image taller than {MAX_TILE_EDGE} must yield multiple rows, got {g:?}");
        assert_eq!(g.rows, 3);
        assert_eq!(g.cols, 1);
        assert_eq!(g.tile_size(0, 2), (1000, 20000 - 2 * 8192));
    }

    #[test]
    fn tile_grid_covers_the_whole_image_exactly() {
        let g = TileGrid::new(25600, 20000);
        let mut covered_w = 0;
        for col in 0..g.cols {
            covered_w += g.tile_size(col, 0).0;
        }
        let mut covered_h = 0;
        for row in 0..g.rows {
            covered_h += g.tile_size(0, row).1;
        }
        assert_eq!((covered_w, covered_h), (25600, 20000));
    }

    #[test]
    fn visible_range_includes_one_tile_margin_and_clips_to_grid() {
        let g = TileGrid::new(25600, 20000); // 4 cols x 3 rows
        // Viewport at origin: cols 0..=1 (margin right), rows 0..=1 (margin below).
        let (c, r) = g.visible_range(Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0))).unwrap();
        assert_eq!((c, r), (0..=1, 0..=1));
        // Viewport in the middle tile (col 1, row 1): margins on both sides.
        let (c, r) = g.visible_range(Rect::from_min_size(Pos2::new(9000.0, 9000.0), Vec2::new(1920.0, 1080.0))).unwrap();
        assert_eq!((c, r), (0..=2, 0..=2));
        // Viewport straddling the far corner: clipped to the last tile.
        let (c, r) = g.visible_range(Rect::from_min_size(Pos2::new(25000.0, 19000.0), Vec2::new(1920.0, 1080.0))).unwrap();
        assert_eq!((c, r), (2..=3, 1..=2));
    }

    #[test]
    fn visible_range_is_none_far_beyond_the_image_edge() {
        let g = TileGrid::new(25600, 720);
        // More than one tile to the right of the image: nothing to draw, no error.
        let far_right = Rect::from_min_size(Pos2::new(25600.0 + 8192.0 * 2.0, 0.0), Vec2::new(1920.0, 1080.0));
        assert!(g.visible_range(far_right).is_none());
        // More than one tile below the image.
        let far_down = Rect::from_min_size(Pos2::new(0.0, 720.0 * 3.0), Vec2::new(1920.0, 1080.0));
        assert!(g.visible_range(far_down).is_none());
    }

    #[test]
    fn tile_origin_is_offset_by_tile_size() {
        let g = TileGrid::new(25600, 20000);
        assert_eq!(g.tile_origin(0, 0), Pos2::ZERO);
        assert_eq!(g.tile_origin(2, 1), Pos2::new(16384.0, 8192.0));
    }
}
