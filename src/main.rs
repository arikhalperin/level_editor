use rust_embed::RustEmbed;
#[derive(RustEmbed)]
#[folder = "src/assets"]
struct Asset;

mod toolbox;
use toolbox::{ToolboxLayout, load_toolbox_layout, render_toolbox, get_toolbox_rect};

mod background;
use background::BackgroundImageController;

mod entities;
use entities::{Entity, DrawableEntity};

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
    scroll_offset: Vec2,
    selected_entity: Option<usize>,
    dragging_entity: bool,
    last_click_time: Option<f64>,
    last_click_pos: Option<Pos2>,
    editing_polygon_entity: Option<usize>,
    editing_polygon_point: Option<usize>,
    dragging_polygon_point: bool,
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
            scroll_offset: Vec2::ZERO,
            selected_entity: None,
            dragging_entity: false,
            last_click_time: None,
            last_click_pos: None,
            editing_polygon_entity: None,
            editing_polygon_point: None,
            dragging_polygon_point: false,
        };

        trace!("init_state_done");
        state
    }
}

impl EditorState {
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
                trace!("load_background_image_success width={} height={} scroll_offset={:?}", width, height, self.scroll_offset);
            }
            Err(e) => {
                error!("load_background_image_error error={}", e);
            }
        }
    }
}

impl eframe::App for EditorState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        trace!("update_start tool={:?} entities={} drawing_points={}", self.tool, self.entities.len(), self.drawing_polygon.as_ref().map(|p| p.len()).unwrap_or(0));
        // Preload tool icons on first frame
        if self.toolbox_layout.is_some() && self.loaded_textures.is_empty() {
            trace!("update_preload_icons");
            self.preload_tool_icons(ctx);
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
                    if ui.button("Load").clicked() {
                        trace!("menu_load_clicked");
                        ui.close_menu();
                    }
                    if ui.button("Save").clicked() {
                        trace!("menu_save_clicked");
                        ui.close_menu();
                    }
                });
            });
        });

        trace!("update_menu_bar_end");

        trace!("update_handle_input_start");
        // Capture screen width and dt once outside the input lock to avoid re-entrance
        let screen_width = ctx.screen_rect().width();
        let dt = ctx.input(|i| i.stable_dt);

        // Handle keyboard input for scrolling
        ctx.input(|i| {
            // Smooth per-frame scroll while a key is held
            let speed = 600.0; // pixels per second
            let step = speed * dt;

            if i.key_down(egui::Key::ArrowLeft) {
                self.scroll_offset.x = (self.scroll_offset.x - step).max(0.0);
                trace!("scroll_left step={} offset={:?}", step, self.scroll_offset);
            }
            if i.key_down(egui::Key::ArrowRight) {
                let max_scroll = (self.background_size.x - screen_width).max(0.0);
                self.scroll_offset.x = (self.scroll_offset.x + step).min(max_scroll);
                trace!("scroll_right step={} offset={:?} max={}", step, self.scroll_offset, max_scroll);
            }
            
            // Finalize polygon with Enter key
            if i.key_pressed(egui::Key::Enter) {
                if let Some(points) = self.drawing_polygon.take() {
                    if points.len() >= 3 {
                        self.entities.push(Entity::new_polygon(points));
                        trace!("polygon_finalized total_entities={}", self.entities.len());
                    } else {
                        trace!("polygon_discarded_too_few_points points={}", points.len());
                    }
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
                        if let Entity::Polygon(ref mut poly) = self.entities[entity_idx] {
                            if poly.points.len() > 3 && point_idx < poly.points.len() {
                                poly.points.remove(point_idx);
                                self.editing_polygon_point = None;
                                trace!("polygon_point_deleted entity={} point={} remaining={}", entity_idx, point_idx, poly.points.len());
                            } else if poly.points.len() <= 3 {
                                trace!("polygon_point_delete_blocked_min_points entity={} points={}", entity_idx, poly.points.len());
                            }
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
            // Preload bitmap texture for preview
            self.load_bitmap_texture(ctx, &selected_name);
            self.tool = match selected_name.as_str() {
                "select_tool" => Tool::Select,
                "delete_tool" => Tool::Erase,
                "blocker_tool" | "polygon_tool" => Tool::DrawPolygon,
                _ => Tool::Bitmap(selected_name.clone()),
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
            trace!("entity_drag_end entity={:?}", self.selected_entity);
        }
        trace!("update_entity_drag_end");

        trace!("update_central_panel_start");
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter();

            // Draw background image
            self.background_controller.draw(ctx, self.scroll_offset.x, painter);

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
                        let rect = egui::Rect::from_min_size(mouse_pos - self.scroll_offset, bitmap_size);
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
                                            self.entities.push(Entity::new_polygon(points));
                                            trace!("polygon_finalized_double_click total_entities={}", self.entities.len());
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
                            _ => {}
                        }
                    }
                }
            }
            
            // Right-click to finalize current polygon
            if secondary_clicked && self.tool == Tool::DrawPolygon {
                if let Some(points) = self.drawing_polygon.take() {
                    if points.len() >= 3 {
                        self.entities.push(Entity::new_polygon(points));
                        trace!("polygon_finalized_right_click total_entities={}", self.entities.len());
                    } else {
                        trace!("polygon_discarded_too_few_points points={}", points.len());
                    }
                }
                // Reset click tracking after closing polygon
                self.last_click_time = None;
                self.last_click_pos = None;
            }
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
        .with_max_inner_size([f32::INFINITY, 720.0]);
    
    let _ = eframe::run_native(
        "Level Map Editor",
        options,
        Box::new(|_cc| Box::new(EditorState::default())),
    );
}
