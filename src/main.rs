use rust_embed::RustEmbed;
#[derive(RustEmbed)]
#[folder = "src/assets"]
struct Asset;

mod level_data;
use level_data::LevelData;

mod toolbox;
use toolbox::{ToolboxLayout, load_toolbox_layout, render_toolbox, get_toolbox_rect};

mod background;
use background::BackgroundImageController;

mod scroll;
use scroll::{HeldDirs, ScrollModel};

mod entities;
use entities::{Entity, DrawableEntity};

mod debug_export;
use debug_export::{write_minimap_debug_json, MinimapDebugSnapshot};

use std::collections::HashMap;
use eframe::egui;
use egui::{Color32, Pos2, Vec2};
use tracing::{error, trace, warn};

#[derive(Clone, PartialEq, Debug)]
enum Tool {
    Select,
    Erase,
    Bitmap(String),
    DrawPolygon,
}

struct EditorState {
    tool: Tool,
    current_tool_name: Option<String>,
    entities: Vec<Entity>,
    drawing_polygon: Option<Vec<Pos2>>,
    _selected_point: Option<(usize, usize)>,
    _drag_offset: Option<Vec2>,
    toolbox_pos: Pos2,
    dragging_toolbox: bool,
    drag_start: Option<Pos2>,
    toolbox_layout: Option<ToolboxLayout>,
    loaded_textures: HashMap<String, egui::TextureHandle>,
    bitmap_sizes: HashMap<String, Vec2>,
    bitmap_textures: HashMap<String, egui::TextureHandle>,
    background_controller: BackgroundImageController,
    background_size: Vec2,
    /// Continuous scroll state (offset + momentum), advanced once per frame.
    scroll: ScrollModel,
    /// The single rounded integer-pixel offset shared by every renderer and
    /// mouse->world conversion this frame: `scroll.pixel_offset()`.
    scroll_offset: Vec2,
    selected_entity: Option<usize>,
    dragging_entity: bool,
    last_click_time: Option<f64>,
    last_click_pos: Option<Pos2>,
    editing_polygon_entity: Option<usize>,
    editing_polygon_point: Option<usize>,
    dragging_polygon_point: bool,
    undo_stack: Vec<Vec<Entity>>,
    redo_stack: Vec<Vec<Entity>>,
    last_save_hash: u64,
    exit_requested: bool,
    last_background_path: Option<std::path::PathBuf>,
    last_level_path: Option<std::path::PathBuf>,
    initial_load_done: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        trace!("init_state_start");
        trace!("embedded_assets_start");
        for file in Asset::iter() {
            trace!("embedded_asset={}", file);
        }
        trace!("embedded_assets_end");
        
        let state = Self {
            tool: Tool::Select,
            current_tool_name: None,
            entities: vec![],
            drawing_polygon: None,
            _selected_point: None,
            _drag_offset: None,
            toolbox_pos: Pos2::new(10.0, 40.0),
            dragging_toolbox: false,
            drag_start: None,
            toolbox_layout: load_toolbox_layout(),
            loaded_textures: HashMap::new(),
            bitmap_sizes: HashMap::new(),
            bitmap_textures: HashMap::new(),
            background_controller: BackgroundImageController::new(),
            background_size: Vec2::ZERO,
            scroll: ScrollModel::default(),
            scroll_offset: Vec2::ZERO,
            selected_entity: None,
            dragging_entity: false,
            last_click_time: None,
            last_click_pos: None,
            editing_polygon_entity: None,
            editing_polygon_point: None,
            dragging_polygon_point: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_save_hash: 0,
            exit_requested: false,
            last_background_path: None,
            last_level_path: None,
            initial_load_done: false,
        };
        
        trace!("init_state_done");
        state
    }
}

impl EditorState {
    fn get_config_file_path() -> std::path::PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("rust_game_editor_config.json")
        } else {
            std::path::PathBuf::from(".rust_game_editor_config.json")
        }
    }
    
    fn load_last_background_path(&mut self) {
        let config_path = Self::get_config_file_path();
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(bg_path) = config.get("last_background").and_then(|v| v.as_str()) {
                    let path = std::path::PathBuf::from(bg_path);
                    if path.exists() {
                        self.last_background_path = Some(path.clone());
                        self.load_background_image(path);
                        trace!("loaded_last_background_image from_config");
                    }
                }
            }
        }
    }
    
    fn save_background_path(&self, path: &std::path::PathBuf) {
        let config_path = Self::get_config_file_path();
        let mut config = if let Ok(content) = std::fs::read_to_string(&config_path) {
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        
        config["last_background"] = serde_json::Value::String(path.to_string_lossy().to_string());
        
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&config_path, json);
            trace!("saved_background_path_to_config path={:?}", path);
        }
    }
    
    fn load_last_level_path(&mut self) {
        let config_path = Self::get_config_file_path();
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(level_path) = config.get("last_level").and_then(|v| v.as_str()) {
                    let path = std::path::PathBuf::from(level_path);
                    if path.exists() {
                        self.last_level_path = Some(path.clone());
                        if let Err(e) = self.load_level_from_path(path) {
                            error!("failed to load last level: {}", e);
                        }
                        trace!("loaded_last_level from_config");
                    }
                }
            }
        }
    }
    
    fn save_level_path(&self, path: &std::path::PathBuf) {
        let config_path = Self::get_config_file_path();
        let mut config = if let Ok(content) = std::fs::read_to_string(&config_path) {
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        
        config["last_level"] = serde_json::Value::String(path.to_string_lossy().to_string());
        
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&config_path, json);
            trace!("saved_level_path_to_config path={:?}", path);
        }
    }
    
    fn compute_entities_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        format!("{:?}", self.entities).hash(&mut hasher);
        hasher.finish()
    }
    
    fn has_unsaved_changes(&self) -> bool {
        self.compute_entities_hash() != self.last_save_hash
    }
    
    fn get_tool_type(&self, tool_name: &str) -> Option<String> {
        self.toolbox_layout.as_ref().and_then(|layout| {
            layout.tools.iter()
                .find(|t| t.name == tool_name)
                .map(|t| t.tool_type.clone())
        })
    }
    
    fn get_tool_color(&self, tool_name: &str) -> Option<String> {
        self.toolbox_layout.as_ref().and_then(|layout| {
            layout.tools.iter()
                .find(|t| t.name == tool_name)
                .and_then(|t| t.color.clone())
        })
    }

    fn save_state(&mut self) {
        // Save current state to undo stack
        self.undo_stack.push(self.entities.clone());
        // Limit undo stack size to prevent memory issues
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        // Clear redo stack when new action is performed
        self.redo_stack.clear();
        trace!("state_saved undo_stack_size={}", self.undo_stack.len());
    }
    
    fn undo(&mut self) {
        if let Some(previous_state) = self.undo_stack.pop() {
            // Save current state to redo stack
            self.redo_stack.push(self.entities.clone());
            // Restore previous state
            self.entities = previous_state;
            // Clear any selections/edits that may now be invalid
            self.selected_entity = None;
            self.editing_polygon_entity = None;
            self.editing_polygon_point = None;
            self.dragging_entity = false;
            self.dragging_polygon_point = false;
            trace!("undo entities={} undo_stack_size={} redo_stack_size={}", self.entities.len(), self.undo_stack.len(), self.redo_stack.len());
        } else {
            trace!("undo_nothing_to_undo");
        }
    }
    
    fn redo(&mut self) {
        if let Some(next_state) = self.redo_stack.pop() {
            // Save current state to undo stack
            self.undo_stack.push(self.entities.clone());
            // Restore next state
            self.entities = next_state;
            // Clear any selections/edits that may now be invalid
            self.selected_entity = None;
            self.editing_polygon_entity = None;
            self.editing_polygon_point = None;
            self.dragging_entity = false;
            self.dragging_polygon_point = false;
            trace!("redo entities={} undo_stack_size={} redo_stack_size={}", self.entities.len(), self.undo_stack.len(), self.redo_stack.len());
        } else {
            trace!("redo_nothing_to_redo");
        }
    }
    
    fn get_bitmap_size(&self, bitmap_name: &str) -> Vec2 {
        if let Some(size) = self.bitmap_sizes.get(bitmap_name) {
            return *size;
        }
        
        // Try to load from toolbox layout to find the actual bitmap asset
        if let Some(layout) = &self.toolbox_layout {
            if let Some(tool_def) = layout.tools.iter().find(|t| &t.name == bitmap_name) {
                let asset_path = tool_def.icon.replace("assets/", "");
                if let Some(bytes) = Asset::get(&asset_path) {
                    if let Ok(img) = image::load_from_memory(bytes.data.as_ref()) {
                        let size = Vec2::new(img.width() as f32, img.height() as f32);
                        return size;
                    }
                }
            }
        }
        Vec2::new(32.0, 32.0) // fallback
    }
    
    fn cache_bitmap_size(&mut self, bitmap_name: &str) {
        if self.bitmap_sizes.contains_key(bitmap_name) {
            return;
        }
        
        let size = self.get_bitmap_size(bitmap_name);
        self.bitmap_sizes.insert(bitmap_name.to_string(), size);
    }

    fn load_bitmap_texture(&mut self, ctx: &egui::Context, bitmap_name: &str) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.bitmap_textures.get(bitmap_name) {
            return Some(tex.clone());
        }
        
        // Try to load from toolbox layout to find the actual bitmap asset
        if let Some(layout) = &self.toolbox_layout {
            if let Some(tool_def) = layout.tools.iter().find(|t| &t.name == bitmap_name) {
                let asset_path = tool_def.icon.replace("assets/", "");
                if let Some(bytes) = Asset::get(&asset_path) {
                    if let Ok(img) = image::load_from_memory(bytes.data.as_ref()) {
                        let size = [img.width() as usize, img.height() as usize];
                        let rgba = img.to_rgba8();
                        let raw = rgba.as_raw();
                        let pixels: Vec<Color32> = raw.chunks(4)
                            .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                            .collect();
                        let image_data = egui::ImageData::Color(egui::ColorImage {
                            size,
                            pixels,
                        }.into());
                        let tex = ctx.load_texture(&format!("bitmap_{}", bitmap_name), image_data, egui::TextureOptions::default());
                        self.bitmap_textures.insert(bitmap_name.to_string(), tex.clone());
                        trace!("loaded_bitmap_texture={} size={:?}", bitmap_name, size);
                        return Some(tex);
                    }
                }
            }
        }
        None
    }

    fn preload_tool_icons(&mut self, ctx: &egui::Context) {
        if let Some(layout) = &self.toolbox_layout {
            trace!("preload_tool_icons_start count={}", layout.tools.len());
            for tool in &layout.tools {
                if !self.loaded_textures.contains_key(&tool.icon) {
                    let asset_path = tool.icon.replace("assets/", "");
                    if let Some(bytes) = Asset::get(&asset_path) {
                        if let Ok(img) = image::load_from_memory(bytes.data.as_ref()) {
                            let size = [img.width() as usize, img.height() as usize];
                            let rgba = img.to_rgba8();
                            let raw = rgba.as_raw();
                            let pixels: Vec<Color32> = raw.chunks(4)
                                .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                                .collect();
                            let image_data = egui::ImageData::Color(egui::ColorImage {
                                size,
                                pixels,
                            }.into());
                            let tex = ctx.load_texture(&tool.icon, image_data, egui::TextureOptions::default());
                            self.loaded_textures.insert(tool.icon.clone(), tex);
                            trace!("preloaded_icon={} size={:?}", tool.icon, size);
                        }
                    } else {
                        warn!("missing_icon_asset={} path={}", tool.icon, asset_path);
                    }
                }
            }
            trace!("preload_tool_icons_end loaded={}", self.loaded_textures.len());
        }
    }

    fn load_background_image(&mut self, path: std::path::PathBuf) {
        trace!("load_background_image_start path={:?}", path);
        match self.background_controller.load_image(&path) {
            Ok((width, height)) => {
                self.background_size = Vec2::new(width as f32, height as f32);
                self.last_background_path = Some(path.clone());
                self.save_background_path(&path);
                trace!("load_background_image_success width={} height={} scroll_offset={:?}", width, height, self.scroll_offset);
            }
            Err(e) => {
                error!("load_background_image_error error={}", e);
            }
        }
    }

    fn save_level(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        trace!("save_level_start entities={}", self.entities.len());
        
        let level_data = LevelData {
            version: "1.0".to_string(),
            background: None, // TODO: Could store background path if needed
            background_size: if self.background_size != Vec2::ZERO {
                Some([self.background_size.x, self.background_size.y])
            } else {
                None
            },
            entities: self.entities.iter().map(|e| e.to_level_entity()).collect(),
        };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("level.json")
            .save_file()
        {
            let json = serde_json::to_string_pretty(&level_data)?;
            std::fs::write(&path, json)?;
            self.last_save_hash = self.compute_entities_hash();
            trace!("save_level_success path={:?} entities={}", path, level_data.entities.len());
        } else {
            trace!("save_level_cancelled");
        }

        Ok(())
    }

    fn load_level_from_path(&mut self, path: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let json = std::fs::read_to_string(&path)?;
        trace!("load_level_from_path read json length={}", json.len());
        
        let level_data: LevelData = match serde_json::from_str(&json) {
            Ok(data) => data,
            Err(e) => {
                error!("load_level_json_parse_error error={}", e);
                return Err(Box::new(e));
            }
        };
        
        trace!("load_level_data version={} entities={}", level_data.version, level_data.entities.len());
        
        // Clear existing entities and load new ones
        self.save_state(); // Save current state to undo stack before clearing
        self.entities.clear();
        
        for level_entity in &level_data.entities {
            let entity = Entity::from_level_entity(level_entity, self.toolbox_layout.as_ref());
            trace!("loaded_entity type={}", entity.entity_type());
            self.entities.push(entity);
        }
        
        // Update background size if available
        if let Some(bg_size) = level_data.background_size {
            self.background_size = Vec2::new(bg_size[0], bg_size[1]);
        }
        
        // Clear selections
        self.selected_entity = None;
        self.editing_polygon_entity = None;
        self.editing_polygon_point = None;
        
        self.last_level_path = Some(path.clone());
        self.save_level_path(&path);
        self.last_save_hash = self.compute_entities_hash();
        
        trace!("load_level_success path={:?} entities={}", path, self.entities.len());
        
        Ok(())
    }

    fn load_level(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        trace!("load_level_start");
        
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        {
            self.load_level_from_path(path)?;
        } else {
            trace!("load_level_cancelled");
        }

        Ok(())
    }
}

/// Draw a bottom-right minimap: full-level thumbnail when a background is loaded, and a rectangle for the visible area.
fn render_level_minimap(
    ctx: &egui::Context,
    bg: &mut BackgroundImageController,
    background_size: Vec2,
    scroll_offset: Vec2,
    entities: &[Entity],
    panel: egui::Rect,
) {
    let timestamp_secs = ctx.input(|i| i.time);
    let pixels_per_point = ctx.input(|i| i.pixels_per_point());
    let screen = ctx.screen_rect();
    if panel.width() < 32.0 || panel.height() < 32.0 {
        write_minimap_debug_json(&MinimapDebugSnapshot::early_only(
            timestamp_secs,
            panel,
            "panel_clip_too_small",
            screen,
            pixels_per_point,
        ));
        return;
    }

    let level = if background_size.x >= 1.0 && background_size.y >= 1.0 {
        background_size
    } else {
        let mut mx = 0.0_f32;
        let mut my = 0.0_f32;
        for e in entities {
            match e {
                Entity::Bitmap(b) => {
                    mx = mx.max(b.pos.x + b.get_size().x);
                    my = my.max(b.pos.y + b.get_size().y);
                }
                Entity::Polygon(p) => {
                    for pt in &p.points {
                        mx = mx.max(pt.x);
                        my = my.max(pt.y);
                    }
                }
            }
        }
        if mx >= 1.0 || my >= 1.0 {
            Vec2::new(mx.max(1.0), my.max(1.0))
        } else {
            // No level bounds yet — use viewport size so the indicator still makes sense.
            Vec2::new(panel.width().max(1.0), panel.height().max(1.0))
        }
    };

    let margin = 16.0;
    let avail_w = (panel.width() - margin * 2.0).max(1.0);
    let avail_h = (panel.height() - margin * 2.0).max(1.0);
    let max_dim = 220.0_f32;
    // Wide levels: initial scale can make one side a hairline; enforce a minimum readable size.
    const MIN_MINIMAP_SIDE: f32 = 100.0;
    let lw = level.x.max(1.0);
    let lh = level.y.max(1.0);
    let scale = (avail_w / lw)
        .min(avail_h / lh)
        .min(max_dim / lw.max(lh));
    let mut map_w = lw * scale;
    let mut map_h = lh * scale;
    let min_side = map_w.min(map_h);
    if min_side < MIN_MINIMAP_SIDE {
        let factor = MIN_MINIMAP_SIDE / min_side;
        map_w *= factor;
        map_h *= factor;
    }
    let clamp = (avail_w / map_w).min(avail_h / map_h).min(1.0);
    map_w *= clamp;
    map_h *= clamp;
    let map_pos = egui::pos2(panel.right() - margin - map_w, panel.bottom() - margin - map_h);
    let mut map_rect = egui::Rect::from_min_size(map_pos, egui::vec2(map_w, map_h));
    map_rect = map_rect.intersect(ctx.screen_rect());
    if map_rect.width() < 4.0 || map_rect.height() < 4.0 {
        return;
    }
    let inner = map_rect.shrink(4.0);

    // `debug_painter` uses `Order::Debug` (above panels, windows, and `Foreground`), with a
    // full-screen clip — this is the most reliable way to ensure the minimap is visible.
    let p = ctx.debug_painter();
    let rounding = 4.0;
    let bg_pale = Color32::from_rgb(238, 238, 240);
    p.rect_filled(map_rect, rounding, bg_pale);
    p.rect_stroke(
        map_rect,
        rounding,
        egui::Stroke::new(1.5, Color32::from_gray(150)),
    );
    let minimap_tex = bg.minimap_texture(ctx);
    let has_minimap_texture = minimap_tex.is_some();
    if let Some(tex) = minimap_tex {
        p.image(
            tex.id(),
            inner,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        p.rect_filled(inner, 0.0, Color32::from_rgb(232, 232, 235));
    }

    let sx = inner.width() / level.x;
    let sy = inner.height() / level.y;
    let scroll_x = scroll_offset.x.max(0.0);
    let scroll_y = scroll_offset.y.max(0.0);
    let vx = inner.min.x + scroll_x * sx;
    let vy = inner.min.y + scroll_y * sy;
    let vw = panel.width() * sx;
    let vh = panel.height() * sy;
    let view_rect = egui::Rect::from_min_size(egui::pos2(vx, vy), egui::vec2(vw, vh)).intersect(inner);
    if view_rect.width() > 0.0 && view_rect.height() > 0.0 {
        p.rect_stroke(view_rect, 0.0, egui::Stroke::new(2.0, Color32::YELLOW));
    }

    write_minimap_debug_json(&MinimapDebugSnapshot::full(
        timestamp_secs,
        panel,
        level,
        map_w,
        map_h,
        map_rect,
        inner,
        has_minimap_texture,
        scroll_offset,
        screen,
        pixels_per_point,
    ));
}

impl eframe::App for EditorState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        trace!("update_start tool={:?} entities={} drawing_points={}", self.tool, self.entities.len(), self.drawing_polygon.as_ref().map(|p| p.len()).unwrap_or(0));
        // Preload tool icons on first frame
        if self.toolbox_layout.is_some() && self.loaded_textures.is_empty() {
            trace!("update_preload_icons");
            self.preload_tool_icons(ctx);
        }
        
        // Load last background and level on first frame after UI is initialized
        if !self.initial_load_done {
            self.initial_load_done = true;
            self.load_last_background_path();
            self.load_last_level_path();
        }

        trace!("update_menu_bar_start");
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Background Image").clicked() {
                        ui.close_menu();
                        // Open file dialog
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG Image", &["png"])
                            .pick_file()
                        {
                            trace!("menu_background_image_selected path={:?}", path);
                            self.load_background_image(path);
                        }
                    }
                    if ui.button("Load Level").clicked() {
                        ui.close_menu();
                        trace!("menu_load_clicked");
                        if let Err(e) = self.load_level() {
                            error!("load_level_error error={}", e);
                        }
                    }
                    if ui.button("Save Level").clicked() {
                        ui.close_menu();
                        trace!("menu_save_clicked");
                        if let Err(e) = self.save_level() {
                            error!("save_level_error error={}", e);
                        }
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ui.close_menu();
                        if self.has_unsaved_changes() {
                            self.exit_requested = true;
                        } else {
                            std::process::exit(0);
                        }
                    }
                });
            });
        });
        
        // Handle exit with unsaved changes prompt
        if self.exit_requested {
            let mut open = true;
            egui::Window::new("Unsaved Changes")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes. Do you want to save before exiting?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and Exit").clicked() {
                            if let Err(e) = self.save_level() {
                                error!("save_level_error error={}", e);
                            }
                            std::process::exit(0);
                        }
                        if ui.button("Exit without Saving").clicked() {
                            std::process::exit(0);
                        }
                        if ui.button("Cancel").clicked() {
                            self.exit_requested = false;
                        }
                    });
                });
            if !open {
                self.exit_requested = false;
            }
        }

        trace!("update_menu_bar_end");

        trace!("update_handle_input_start");
        // Two-axis arrow-key scrolling with momentum. Arrow keys are ignored while a
        // text field owns the keyboard so text entry keeps working.
        let dt = ctx.input(|i| i.stable_dt);
        let held = if ctx.wants_keyboard_input() {
            HeldDirs::default()
        } else {
            ctx.input(|i| HeldDirs {
                left: i.key_down(egui::Key::ArrowLeft),
                right: i.key_down(egui::Key::ArrowRight),
                up: i.key_down(egui::Key::ArrowUp),
                down: i.key_down(egui::Key::ArrowDown),
            })
        };
        self.scroll.step(held, dt);
        self.scroll_offset = self.scroll.pixel_offset();
        if self.scroll.needs_repaint(held) {
            // Keep advancing at display rate instead of OS key-repeat rate.
            ctx.request_repaint();
            trace!(
                "scroll_step held={:?} dt={} offset={:?} velocity={:?} pixel_offset={:?}",
                held, dt, self.scroll.offset, self.scroll.velocity, self.scroll_offset
            );
        }

        ctx.input(|i| {

            // Arrow-key scrolling is handled by the ScrollModel step above.
            
            // Finalize polygon with Enter key
            if i.key_pressed(egui::Key::Enter) {
                if let Some(points) = self.drawing_polygon.take() {
                    if points.len() >= 3 {
                        self.save_state();
                        // Determine polygon type and color from current tool
                        let polygon_type = self.current_tool_name.as_deref().unwrap_or("polygon").to_string();
                        let color = self.get_tool_color(&polygon_type);
                        self.entities.push(Entity::new_polygon_with_type_and_color(points, polygon_type, color));
                        trace!("polygon_finalized total_entities={}", self.entities.len());
                    } else {
                        trace!("polygon_discarded_too_few_points points={}", points.len());
                    }
                }
            }
            
            // Undo with Cmd+Z (Ctrl+Z on Windows/Linux)
            if i.key_pressed(egui::Key::Z) && i.modifiers.command {
                if i.modifiers.shift {
                    // Redo with Shift+Cmd+Z (Shift+Ctrl+Z on Windows/Linux)
                    self.redo();
                } else {
                    // Undo with Cmd+Z (Ctrl+Z on Windows/Linux)
                    self.undo();
                }
            }
            
            // Cancel polygon or exit polygon edit mode with Escape key
            if i.key_pressed(egui::Key::Escape) {
                if self.drawing_polygon.is_some() {
                    self.drawing_polygon = None;
                    trace!("polygon_cancelled");
                } else if self.editing_polygon_entity.is_some() {
                    self.editing_polygon_entity = None;
                    self.editing_polygon_point = None;
                    self.dragging_polygon_point = false;
                    self.selected_entity = None;
                    trace!("polygon_edit_mode_cancelled");
                }
            }
            
            // Delete selected polygon point with Delete or Backspace key
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                if let (Some(entity_idx), Some(point_idx)) = (self.editing_polygon_entity, self.editing_polygon_point) {
                    if entity_idx < self.entities.len() {
                        let can_delete = if let Entity::Polygon(ref poly) = self.entities[entity_idx] {
                            poly.points.len() > 3 && point_idx < poly.points.len()
                        } else {
                            false
                        };

                        if can_delete {
                            self.save_state();
                            if let Entity::Polygon(ref mut poly) = self.entities[entity_idx] {
                                poly.points.remove(point_idx);
                                self.editing_polygon_point = None;
                                trace!("polygon_point_deleted entity={} point={} remaining={}", entity_idx, point_idx, poly.points.len());
                            }
                        } else if let Entity::Polygon(ref poly) = self.entities[entity_idx] {
                            trace!("polygon_point_delete_blocked_min_points entity={} points={}", entity_idx, poly.points.len());
                        }
                    }
                }
            }

        });
        trace!("update_handle_input_end");

        let pointer = ctx.input(|i| i.pointer.clone());
        let pointer_pos = pointer.interact_pos();
        let pointer_pressed = pointer.primary_down();
        let pointer_clicked = pointer.primary_clicked();
        let pointer_released = pointer.primary_released();
        let secondary_clicked = pointer.secondary_clicked();

        trace!("update_toolbox_start toolbox_pos={:?} dragging={}", self.toolbox_pos, self.dragging_toolbox);
        // Render toolbox and get tool selection
        let mut tool_selected = None;
        if let Some(layout) = &self.toolbox_layout {
            tool_selected = render_toolbox(ctx, layout, self.toolbox_pos, &mut self.loaded_textures, &self.current_tool_name);
        }
        trace!("update_toolbox_end tool_selected={:?}", tool_selected);
        
        trace!("update_handle_toolbox_drag_start");
        // Drag logic for toolbox
        if pointer_pressed && self.toolbox_layout.is_some() {
            let layout = self.toolbox_layout.as_ref().unwrap();
            let toolbox_rect = get_toolbox_rect(layout, self.toolbox_pos);
            
            if let Some(pos) = pointer_pos {
                if toolbox_rect.contains(pos) {
                    if !self.dragging_toolbox {
                        self.dragging_toolbox = true;
                        self.drag_start = pointer_pos;
                        trace!("toolbox_drag_start pos={:?}", pos);
                    }
                }
            }
            
            if self.dragging_toolbox {
                if let (Some(start), Some(current)) = (self.drag_start, pointer_pos) {
                    let delta = current - start;
                    self.toolbox_pos += delta;
                    self.drag_start = Some(current);
                    trace!("toolbox_drag_move delta={:?} new_pos={:?}", delta, self.toolbox_pos);
                }
            }
        } else {
            if !pointer_pressed {
                self.dragging_toolbox = false;
                self.drag_start = None;
                trace!("toolbox_drag_end");
            }
        }
        trace!("update_handle_toolbox_drag_end");
        // Set tool based on selection


        trace!("update_handle_tool_selection_start");
        if let Some(selected_name) = tool_selected {
            trace!("tool_selected name={}", selected_name);
            self.current_tool_name = Some(selected_name.clone());
            
            // Determine tool type based on JSON configuration
            let tool_type = self.get_tool_type(&selected_name);
            trace!("tool_type from_json={:?}", tool_type);
            
            self.tool = match tool_type.as_deref() {
                Some("tool") => {
                    // Check specific tool names for special tools
                    match selected_name.as_str() {
                        "select_tool" => Tool::Select,
                        "delete_tool" => Tool::Erase,
                        _ => Tool::Select, // Default for unknown tool types
                    }
                }
                Some("polygon") => Tool::DrawPolygon,
                Some("bitmap") | _ => {
                    // Preload bitmap texture for preview
                    self.load_bitmap_texture(ctx, &selected_name);
                    Tool::Bitmap(selected_name.clone())
                }
            };
        }

        trace!("update_handle_tool_selection_end current_tool={:?}", self.tool);

        trace!("update_entity_drag_start dragging={} selected={:?}", self.dragging_entity, self.selected_entity);
        // Handle polygon point dragging
        if self.dragging_polygon_point && pointer_pressed {
            if let (Some(pos), Some(entity_idx), Some(point_idx)) = (pointer_pos, self.editing_polygon_entity, self.editing_polygon_point) {
                if entity_idx < self.entities.len() {
                    if let Entity::Polygon(ref mut poly) = self.entities[entity_idx] {
                        if point_idx < poly.points.len() {
                            let world_pos = pos + self.scroll_offset;
                            poly.points[point_idx] = world_pos;
                            trace!("polygon_point_drag entity={} point={} pos={:?}", entity_idx, point_idx, world_pos);
                        }
                    }
                }
            }
        }
        
        if pointer_released && self.dragging_polygon_point {
            self.dragging_polygon_point = false;
            // Save state after polygon point was moved
            if self.editing_polygon_entity.is_some() && self.editing_polygon_point.is_some() {
                self.save_state();
            }
            trace!("polygon_point_drag_end");
        }
        
        // Handle entity dragging
        if self.dragging_entity && pointer_pressed {
            if let (Some(pos), Some(entity_idx)) = (pointer_pos, self.selected_entity) {
                if entity_idx < self.entities.len() {
                    // Initialize drag_start if not set
                    if self.drag_start.is_none() {
                        self.drag_start = Some(pos);
                    }
                    
                    // Calculate the delta from the last frame
                    if let Some(last_pos) = self.drag_start {
                        let frame_delta = pos - last_pos;
                        if frame_delta.length_sq() > 0.0 {
                            self.entities[entity_idx].translate(frame_delta);
                            trace!("entity_drag_update idx={} delta={:?}", entity_idx, frame_delta);
                        }
                        self.drag_start = Some(pos);
                    }
                }
            }
        }
        
        if pointer_released && self.dragging_entity {
            self.dragging_entity = false;
            self._drag_offset = None;
            self.drag_start = None;
            // Save state after entity was moved
            if self.selected_entity.is_some() {
                self.save_state();
            }
            trace!("entity_drag_end entity={:?}", self.selected_entity);
        }
        trace!("update_entity_drag_end");

        trace!("update_central_panel_start");
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();

            // Draw background image
            self.background_controller.draw(ctx, self.scroll_offset, ui.clip_rect(), painter);

            // Draw current drawing polygon
            if let Some(points) = &self.drawing_polygon {
                for i in 0..points.len().saturating_sub(1) {
                    let p1 = points[i] - self.scroll_offset;
                    let p2 = points[i + 1] - self.scroll_offset;
                    painter.line_segment([p1, p2], (2.0, Color32::LIGHT_BLUE));
                }
                for p in points {
                    painter.circle_filled(*p - self.scroll_offset, 4.0, Color32::LIGHT_RED);
                }
            }

            // Collect bitmap names to preload textures
            let bitmap_names: Vec<String> = self.entities.iter()
                .filter_map(|e| e.as_bitmap().map(|b| b.bitmap_name.clone()))
                .collect();
            
            // Preload bitmap textures
            for name in bitmap_names {
                self.load_bitmap_texture(ctx, &name);
            }

            // Draw all entities
            for (idx, entity) in self.entities.iter().enumerate() {
                let is_selected = self.selected_entity == Some(idx);
                let is_editing = self.editing_polygon_entity == Some(idx);
                
                // Special handling for bitmap entities (need textures)
                if let Some(bitmap_entity) = entity.as_bitmap() {
                    if let Some(texture_handle) = self.bitmap_textures.get(&bitmap_entity.bitmap_name) {
                        let bitmap_size = bitmap_entity.get_size();
                        let rect = egui::Rect::from_min_size(bitmap_entity.pos - self.scroll_offset, bitmap_size);
                        ui.painter().image(
                            texture_handle.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                            Color32::WHITE
                        );
                        
                        if is_selected {
                            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, Color32::GREEN));
                        }
                    }
                } else {
                    // For polygon entities, use the trait method
                    entity.draw(painter, self.scroll_offset, is_selected);
                    
                    // If editing this polygon, draw points with larger markers
                    if is_editing {
                        if let Some(polygon) = entity.as_polygon() {
                            for (point_idx, point) in polygon.points.iter().enumerate() {
                                let screen_pos = *point - self.scroll_offset;
                                let is_selected_point = self.editing_polygon_point == Some(point_idx);
                                let point_color = if is_selected_point { Color32::YELLOW } else { Color32::WHITE };
                                let point_radius = if is_selected_point { 6.0 } else { 5.0 };
                                painter.circle_filled(screen_pos, point_radius, point_color);
                                painter.circle_stroke(screen_pos, point_radius, (1.0, Color32::BLACK));
                            }
                        }
                    }
                }
            }

            // Draw bitmap preview at mouse position when bitmap tool selected
            if let Some(ref tool_name) = self.current_tool_name {
                if let Some(mouse_pos) = pointer_pos {
                    if let Some(texture_handle) = self.bitmap_textures.get(tool_name) {
                        let bitmap_size = self.get_bitmap_size(tool_name);
                        // `mouse_pos` is already in screen space. A click places the bitmap at
                        // world `mouse_pos + scroll_offset`, which draws back at screen
                        // `(mouse_pos + scroll_offset) - scroll_offset == mouse_pos`, so the
                        // ghost must sit exactly under the cursor for the shared offset to hold.
                        let ghost_world = mouse_pos + self.scroll_offset;
                        let rect = egui::Rect::from_min_size(ghost_world - self.scroll_offset, bitmap_size);
                        // Draw with 50% opacity (semi-transparent)
                        let semi_transparent = Color32::from_rgba_unmultiplied(255, 255, 255, 128);
                        ui.painter().image(
                            texture_handle.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                            semi_transparent
                        );
                    }
                }
            }

            // Mouse handling (simplified)
            if pointer_clicked {
                if let Some(pos) = pointer_pos {
                    // Check if click is within toolbox bounds
                    let toolbox_click = if let Some(layout) = &self.toolbox_layout {
                        let toolbox_rect = get_toolbox_rect(layout, self.toolbox_pos);
                        toolbox_rect.contains(pos)
                    } else {
                        false
                    };
                    
                    if !toolbox_click {
                        // Adjust position for scroll offset
                        let world_pos = pos + self.scroll_offset;
                        
                        // Detect double-click
                        let current_time = ctx.input(|i| i.time);
                        let is_double_click = if let (Some(last_time), Some(last_pos)) = (self.last_click_time, self.last_click_pos) {
                            let time_diff = current_time - last_time;
                            let distance = (world_pos - last_pos).length();
                            time_diff < 0.3 && distance < 10.0  // 300ms and 10 pixels threshold
                        } else {
                            false
                        };
                        
                        match &self.tool {
                            Tool::DrawPolygon => {
                                if is_double_click {
                                    // Double-click detected, close the polygon
                                    if let Some(points) = self.drawing_polygon.take() {
                                        if points.len() >= 3 {
                                            self.save_state();
                                            // Determine polygon type and color from current tool
                                            let polygon_type = self.current_tool_name.as_deref().unwrap_or("polygon").to_string();
                                            let color = self.get_tool_color(&polygon_type);
                                            trace!("polygon_finalized_double_click type={} total_entities={}", polygon_type, self.entities.len() + 1);
                                            self.entities.push(Entity::new_polygon_with_type_and_color(points, polygon_type, color));
                                        } else {
                                            trace!("polygon_discarded_too_few_points points={}", points.len());
                                        }
                                    }
                                    // Reset click tracking to prevent triple-click issues
                                    self.last_click_time = None;
                                    self.last_click_pos = None;
                                } else {
                                    // Single click - add point to polygon
                                    self.drawing_polygon.get_or_insert(vec![]).push(world_pos);
                                    trace!("draw_polygon_add_point pos={:?} total={}", world_pos, self.drawing_polygon.as_ref().map(|p| p.len()).unwrap_or(0));
                                    // Update click tracking
                                    self.last_click_time = Some(current_time);
                                    self.last_click_pos = Some(world_pos);
                                }
                            }
                            Tool::Bitmap(_) => {
                                let bitmap_name = match &self.tool {
                                    Tool::Bitmap(name) => name.clone(),
                                    _ => unreachable!(),
                                };
                                self.save_state();
                                self.entities.push(Entity::new_bitmap(world_pos, bitmap_name, self.toolbox_layout.as_ref()));
                                trace!("spawn_entity pos={:?} total={}", world_pos, self.entities.len());
                                // Update click tracking for non-polygon tools
                                self.last_click_time = Some(current_time);
                                self.last_click_pos = Some(world_pos);
                            }
                            Tool::Select => {
                                // If in polygon edit mode, check for point clicks first
                                let mut handle_normal_selection = true;
                                if let Some(editing_idx) = self.editing_polygon_entity {
                                    if editing_idx < self.entities.len() {
                                        if let Entity::Polygon(ref poly) = self.entities[editing_idx] {
                                            let mut clicked_point = None;
                                            // Check if clicking on any point (larger hit area)
                                            for (point_idx, point) in poly.points.iter().enumerate() {
                                                if (world_pos - *point).length() < 8.0 {
                                                    clicked_point = Some(point_idx);
                                                    break;
                                                }
                                            }
                                            
                                            if let Some(point_idx) = clicked_point {
                                                // Clicked on a point - select and start dragging it
                                                self.editing_polygon_point = Some(point_idx);
                                                self.dragging_polygon_point = true;
                                                trace!("polygon_point_selected entity={} point={}", editing_idx, point_idx);
                                                handle_normal_selection = false;
                                            } else if !poly.points.iter().any(|p| (world_pos - *p).length() < 8.0) && !self.entities[editing_idx].contains_point(world_pos) {
                                                // Clicked outside polygon - exit edit mode
                                                self.editing_polygon_entity = None;
                                                self.editing_polygon_point = None;
                                                self.selected_entity = None;
                                                trace!("exit_polygon_edit_mode");
                                                handle_normal_selection = false;
                                            } else {
                                                handle_normal_selection = false;
                                            }
                                            // Update click tracking
                                            self.last_click_time = Some(current_time);
                                            self.last_click_pos = Some(world_pos);
                                        }
                                    }
                                }
                                
                                if handle_normal_selection {
                                
                                // Check if clicking on any entity (iterate backwards for top-most first)
                                let mut clicked_entity = None;
                                for (idx, entity) in self.entities.iter().enumerate().rev() {
                                    if entity.contains_point(world_pos) {
                                        clicked_entity = Some(idx);
                                        break;
                                    }
                                }
                                
                                if let Some(idx) = clicked_entity {
                                    // Check for double-click on polygon to enter edit mode
                                    if is_double_click && matches!(self.entities[idx], Entity::Polygon(_)) {
                                        self.editing_polygon_entity = Some(idx);
                                        self.editing_polygon_point = None;
                                        self.selected_entity = Some(idx);
                                        self.dragging_entity = false;
                                        trace!("enter_polygon_edit_mode idx={}", idx);
                                        // Reset click tracking to prevent further processing
                                        self.last_click_time = None;
                                        self.last_click_pos = None;
                                    } else {
                                        // Single click - normal selection and dragging
                                        self.selected_entity = Some(idx);
                                        self.dragging_entity = true;
                                        self.drag_start = pointer_pos;
                                        self.editing_polygon_entity = None;
                                        self.editing_polygon_point = None;
                                        trace!("entity_drag_start idx={} type={}", idx, self.entities[idx].entity_type());
                                        // Update click tracking for select tool
                                        self.last_click_time = Some(current_time);
                                        self.last_click_pos = Some(world_pos);
                                    }
                                } else {
                                    self.selected_entity = None;
                                    self.dragging_entity = false;
                                    self.editing_polygon_entity = None;
                                    self.editing_polygon_point = None;
                                    trace!("deselect_all");
                                    // Update click tracking for select tool
                                    self.last_click_time = Some(current_time);
                                    self.last_click_pos = Some(world_pos);
                                }
                                }
                            }
                            Tool::Erase => {
                                // Check if clicking on any entity to delete it
                                let mut clicked_entity = None;
                                for (idx, entity) in self.entities.iter().enumerate().rev() {
                                    if entity.contains_point(world_pos) {
                                        clicked_entity = Some(idx);
                                        break;
                                    }
                                }
                                
                                if let Some(idx) = clicked_entity {
                                    let entity_type = self.entities[idx].entity_type().to_string();
                                    self.save_state();
                                    self.entities.remove(idx);
                                    // Clear selection if we deleted the selected entity
                                    if self.selected_entity == Some(idx) {
                                        self.selected_entity = None;
                                    } else if let Some(sel_idx) = self.selected_entity {
                                        // Adjust selection index if we deleted an entity before it
                                        if idx < sel_idx {
                                            self.selected_entity = Some(sel_idx - 1);
                                        }
                                    }
                                    // Clear edit mode if we deleted the entity being edited
                                    if self.editing_polygon_entity == Some(idx) {
                                        self.editing_polygon_entity = None;
                                        self.editing_polygon_point = None;
                                        self.dragging_polygon_point = false;
                                    } else if let Some(edit_idx) = self.editing_polygon_entity {
                                        // Adjust edit index if we deleted an entity before it
                                        if idx < edit_idx {
                                            self.editing_polygon_entity = Some(edit_idx - 1);
                                        }
                                    }
                                    trace!("entity_deleted idx={} type={} remaining={}", idx, entity_type, self.entities.len());
                                }
                                // Update click tracking
                                self.last_click_time = Some(current_time);
                                self.last_click_pos = Some(world_pos);
                            }
                        }
                    }
                }
            }
            
            // Right-click to finalize current polygon
            if secondary_clicked && self.tool == Tool::DrawPolygon {
                if let Some(points) = self.drawing_polygon.take() {
                    if points.len() >= 3 {
                        self.save_state();
                        // Determine polygon type and color from current tool
                        let polygon_type = self.current_tool_name.as_deref().unwrap_or("polygon").to_string();
                        let color = self.get_tool_color(&polygon_type);
                        self.entities.push(Entity::new_polygon_with_type_and_color(points, polygon_type, color));
                        trace!("polygon_finalized_right_click total_entities={}", self.entities.len());
                    } else {
                        trace!("polygon_discarded_too_few_points points={}", points.len());
                    }
                }
                // Reset click tracking after closing polygon
                self.last_click_time = None;
                self.last_click_pos = None;
            }

            render_level_minimap(
                ctx,
                &mut self.background_controller,
                self.background_size,
                self.scroll_offset,
                &self.entities,
                ui.clip_rect(),
            );
        });
        trace!("update_central_panel_end");

        trace!("update_end scroll_offset={:?} toolbox_pos={:?}", self.scroll_offset, self.toolbox_pos);
    }
}

fn init_tracing() {
    // Prefer env filter for control, fallback to TRACE for everything
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("trace"));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .compact()
        .finish();

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("failed to set global tracing subscriber: {}", e);
    }
}

fn main() {
    init_tracing();
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_min_inner_size([800.0, 720.0])
        .with_maximized(true);
    
    let _ = eframe::run_native(
        "Level Map Editor",
        options,
        Box::new(|_cc| Box::new(EditorState::default())),
    );
}
