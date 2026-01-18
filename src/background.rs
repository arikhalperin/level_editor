use eframe::egui;
use egui::Color32;
use tracing::{error, trace};

pub const SCREEN_HEIGHT: u32 = 720;

pub struct BackgroundImageController {
    // Full image stored in memory
    full_image: Option<image::DynamicImage>,
    // Three textures: left, center, right with their X positions
    textures: [Option<(egui::TextureHandle, u32)>; 3],
    // Last tile index loaded (tile is one screen wide)
    last_tile: u32,
}

impl BackgroundImageController {
    pub fn new() -> Self {
        Self {
            full_image: None,
            textures: [None, None, None],
            last_tile: 0,
        }
    }
    
    pub fn load_image(&mut self, path: &std::path::PathBuf) -> Result<(u32, u32), String> {
        trace!("background_load_image_start path={:?}", path);
        match image::open(path) {
            Ok(img) => {
                let width = img.width();
                let height = img.height();
                self.full_image = Some(img);
                trace!("background_load_image_success width={} height={}", width, height);
                Ok((width, height))
            }
            Err(e) => {
                error!("background_load_image_error error={}", e);
                Err(format!("Failed to load image: {}", e))
            }
        }
    }
    
    pub fn draw(&mut self, ctx: &egui::Context, screen_x: f32, painter: &egui::Painter) {
        if self.full_image.is_none() {
            trace!("background_draw_skip full_image_missing");
            return;
        }
        
        let screen_x_u32 = screen_x.max(0.0) as u32;
        let screen_width = ctx.screen_rect().width() as u32;
        let tile_width = screen_width.max(1); // avoid div by zero

        let tile_idx = screen_x_u32 / tile_width;
        let tile_offset = (screen_x_u32 % tile_width) as f32;

        // Update textures only when crossing tile boundaries (one screen width)
        if self.textures[1].is_none() || tile_idx != self.last_tile {
            trace!("background_tile_change tile_idx={} last_tile={} tile_width={}", tile_idx, self.last_tile, tile_width);
            self.update_textures(ctx, tile_idx, tile_width);
            self.last_tile = tile_idx;
        }

        // Draw visible tiles with horizontal offsets for smooth movement
        let tile_width_f = tile_width as f32;
        let targets = [
            (-tile_width_f - tile_offset, 0), // left
            (-tile_offset, 1),                // center
            (tile_width_f - tile_offset, 2),  // right
        ];

        for (dest_x, slot) in targets {
            if let Some((tex, _x)) = &self.textures[slot] {
                let rect = egui::Rect::from_min_size(
                    egui::Pos2::new(dest_x, 0.0),
                    egui::vec2(tile_width_f, SCREEN_HEIGHT as f32),
                );
                painter.image(
                    tex.into(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }
    
    fn update_textures(&mut self, ctx: &egui::Context, tile_idx: u32, tile_width: u32) {
        if let Some(full_img) = &self.full_image {
            trace!("background_update_textures_start tile_idx={} tile_width={}", tile_idx, tile_width);
            let base_x = tile_idx.saturating_mul(tile_width);
            let desired = [
                base_x.saturating_sub(tile_width), // left
                base_x,                             // center
                base_x + tile_width,                // right
            ];

            let mut new_textures: [Option<(egui::TextureHandle, u32)>; 3] = [None, None, None];

            for (i, &x_offset) in desired.iter().enumerate() {
                // Reuse existing texture if already loaded for this x_offset
                if let Some(existing) = self.textures.iter().find(|t| t.as_ref().map(|(_, x)| *x == x_offset).unwrap_or(false)) {
                    new_textures[i] = existing.clone();
                } else {
                    // Load only the missing tile (at most one per boundary crossing)
                    new_textures[i] = self.load_tile(ctx, x_offset, tile_width, full_img);
                }
            }

            self.textures = new_textures;
            trace!("background_update_textures_end tile_idx={}", tile_idx);
        }
    }
    
    fn load_tile(&self, ctx: &egui::Context, x: u32, width: u32, full_img: &image::DynamicImage) -> Option<(egui::TextureHandle, u32)> {
        let crop_width = width.min(full_img.width().saturating_sub(x));
        let crop_height = SCREEN_HEIGHT.min(full_img.height());
        
        if crop_width == 0 || crop_height == 0 {
            trace!("background_load_tile_skip x={} width={} crop_width={} crop_height={}", x, width, crop_width, crop_height);
            return None;
        }
        
        let cropped = image::imageops::crop_imm(full_img, x, 0, crop_width, crop_height);
        let cropped_dynamic = image::DynamicImage::ImageRgba8(cropped.to_image());
        let rgba = cropped_dynamic.to_rgba8();
        let pixels: Vec<Color32> = rgba.as_raw().chunks(4)
            .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
            .collect();
        
        let image_data = egui::ImageData::Color(egui::ColorImage {
            size: [crop_width as usize, crop_height as usize],
            pixels,
        }.into());
        
        let tex = ctx.load_texture(&format!("bg_tile_{}", x), image_data, egui::TextureOptions::default());
        trace!("background_load_tile_success x={} width={} height={}", x, crop_width, crop_height);
        Some((tex, x))
    }
}
