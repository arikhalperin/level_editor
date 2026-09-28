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

mod minimap;
use minimap::MinimapLayout;

mod camera;

mod new_level;
use new_level::BlankLevel;

mod help;
use help::{escape_action, escape_closes_help, EscapeAction, HelpContent};

mod game_config;

mod level_size;

mod sim;
use sim::{CollisionPoly, World};

mod combat;
use combat::{BitmapSpawn, PlayInput, PlaySession};

mod entities;
use entities::{Entity, DrawableEntity, RopeEntity, RopePlacement, ROPE_COLOR};

mod debug_export;

mod undo;
use undo::Snapshot;

mod ai_client;

mod traversal;

mod level_gen;

mod repair;

mod generate_ui;
use generate_ui::{GenerateDialog, GenerateRun};
use debug_export::{write_minimap_debug_json, MinimapDebugSnapshot};

use std::collections::HashMap;
use eframe::egui;
use egui::{Color32, Pos2, Vec2};
use tracing::{error, trace, warn};

/// A command that destroys the current level and so must ask about unsaved work first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PendingAction {
    Exit,
    NewLevel,
    /// Replace the level with a generated one, which destroys the open level just as
    /// `NewLevel` does and so asks about unsaved work the same way.
    GenerateLevel,
}

impl PendingAction {
    /// How the confirmation describes what is about to happen.
    fn prompt(self) -> &'static str {
        match self {
            PendingAction::Exit => "You have unsaved changes. Save before exiting?",
            PendingAction::NewLevel => {
                "You have unsaved changes. Save before starting a new level?"
            }
            PendingAction::GenerateLevel => {
                "You have unsaved changes. Save before generating a new level?"
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Tool {
    Select,
    Erase,
    Bitmap(String),
    DrawPolygon,
    Rope,
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
    /// The canvas rectangle as it was laid out last frame, excluding the menu bar.
    /// With `scroll_offset` it gives the visible world region, whose centre is where
    /// play starts the character. `None` until the first frame has been laid out.
    canvas_rect: Option<egui::Rect>,
    selected_entity: Option<usize>,
    dragging_entity: bool,
    last_click_time: Option<f64>,
    last_click_pos: Option<Pos2>,
    editing_polygon_entity: Option<usize>,
    editing_polygon_point: Option<usize>,
    dragging_polygon_point: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    last_save_hash: u64,
    /// Which command is waiting on the unsaved-changes confirmation, if any.
    pending_action: Option<PendingAction>,
    last_background_path: Option<std::path::PathBuf>,
    last_level_path: Option<std::path::PathBuf>,
    initial_load_done: bool,
    /// Whether the Keyboard & Commands window is showing. View state only: never
    /// serialised and never part of `has_unsaved_changes`.
    help_open: bool,
    /// Screen rect of the help window while it is open, so clicks inside it are
    /// consumed by the window instead of falling through to the canvas.
    help_rect: Option<egui::Rect>,
    /// Screen rect of the Level Size dialog while it is open, so clicks inside it are
    /// consumed by the dialog instead of falling through to the canvas.
    size_dialog_rect: Option<egui::Rect>,
    /// Screen rect of the top menu bar this frame.
    menu_bar_rect: Option<egui::Rect>,
    /// Set when a menu item was activated this frame, so the click that chose it is
    /// never also delivered to the canvas beneath the (now closed) dropdown.
    ui_consumed_click: bool,
    /// Where the minimap (navigation box) sits this frame, shared by its renderer and its
    /// hit-test so the drawn box and the clickable box are the same rectangle.
    minimap_layout: Option<MinimapLayout>,
    /// True from a left-button press on the minimap until release: every frame in between
    /// re-centres the view on the pointer.
    minimap_drag: bool,
    /// Anchor of the rope being placed (first click done, second pending).
    drawing_rope: Option<Pos2>,
    /// Explicit level extent, when one has been set. Saved to the level JSON.
    level_size: Option<Vec2>,
    /// Open state and in-progress text of the Level Size dialog.
    size_dialog: Option<(String, String)>,
    /// The running play simulation. `None` means the editor is not in play mode.
    play: Option<PlaySession>,
    /// Where the character spawns, in world space: the centre of the visible canvas
    /// when play started, or wherever the canvas was last clicked during play. An
    /// in-run restart and a death both return the character here.
    play_spawn: Option<Pos2>,
    /// Where a run of the open level begins, when the level file carries one. Play mode
    /// starts the character here instead of at the centre of the visible canvas; a level
    /// with none — every level built by hand — keeps the view-centre behaviour.
    level_spawn: Option<Pos2>,
    /// Where the open level's critical path ends, when it carries one. Written by AI
    /// generation so the route it proved can be re-proved later.
    level_exit: Option<Pos2>,
    /// Where the config lives, when it is not the user's config directory. Only a test sets
    /// this.
    config_path_override: Option<std::path::PathBuf>,
    /// The generation dialog's contents. Kept between openings so the endpoint, the model
    /// name and the last prompt are still there next time.
    generate_dialog: GenerateDialog,
    /// True while the generation window is open.
    generate_open: bool,
    /// The run in flight, if any.
    generate_run: Option<GenerateRun>,
    /// What to tell the user about the run that just finished: the report, or the failure.
    generate_message: Option<String>,
    /// Tool and tool name to restore when play mode ends.
    tool_before_play: Option<(Tool, Option<String>)>,
    /// The editor's scroll state when play began, restored when play ends so testing a
    /// level does not leave the canvas parked wherever the character finished.
    view_before_play: Option<ScrollModel>,
    /// Whether the combat debug view (slash hitbox, orc ranges) is drawn. Off by default.
    combat_debug: bool,
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
            canvas_rect: None,
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
            pending_action: None,
            last_background_path: None,
            last_level_path: None,
            initial_load_done: false,
            help_open: false,
            help_rect: None,
            size_dialog_rect: None,
            menu_bar_rect: None,
            ui_consumed_click: false,
            minimap_layout: None,
            minimap_drag: false,
            drawing_rope: None,
            level_size: None,
            size_dialog: None,
            play: None,
            play_spawn: None,
            level_spawn: None,
            level_exit: None,
            config_path_override: None,
            generate_dialog: GenerateDialog::default(),
            generate_open: false,
            generate_run: None,
            generate_message: None,
            tool_before_play: None,
            view_before_play: None,
            combat_debug: false,
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

    /// Where this editor reads and writes its config. Normally the user's config directory;
    /// a test points it at a file of its own, so what the editor remembers across a restart
    /// can actually be checked instead of only assumed.
    fn config_file_path(&self) -> std::path::PathBuf {
        self.config_path_override
            .clone()
            .unwrap_or_else(Self::get_config_file_path)
    }
    
    fn load_last_background_path(&mut self) {
        let config_path = self.config_file_path();
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
        let config_path = self.config_file_path();
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
        let config_path = self.config_file_path();
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
        let config_path = self.config_file_path();
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
        new_level::level_hash(&self.entities, self.level_size, self.level_spawn, self.level_exit)
    }

    /// Throw the level away and start an empty one. Pure in-memory: nothing is written,
    /// deleted or overwritten on disk.
    fn new_level(&mut self) {
        let blank = BlankLevel::new();
        self.entities = blank.entities;
        self.level_size = blank.level_size;
        self.background_size = blank.background_size;
        self.background_controller.clear();
        self.play_spawn = blank.play_spawn;
        self.level_spawn = blank.level_spawn;
        self.level_exit = blank.level_exit;
        self.last_click_pos = None;
        self.selected_entity = blank.selected_entity;
        self.editing_polygon_entity = blank.editing_polygon_entity;
        self.editing_polygon_point = blank.editing_polygon_point;
        self.dragging_entity = false;
        self.dragging_polygon_point = false;
        self.minimap_drag = false;
        self.drawing_polygon = None;
        self.drawing_rope = None;
        self.undo_stack = blank.undo_stack;
        self.redo_stack = blank.redo_stack;
        self.scroll = blank.scroll;
        self.scroll_offset = self.scroll.pixel_offset();
        // The Level Size window is reachable from the menu bar while it is open; leaving
        // it up would let its OK re-apply the old level's explicit size to the new one.
        self.size_dialog = None;
        self.last_level_path = blank.last_level_path;
        self.last_background_path = blank.last_background_path;
        // Marked clean from the blank content itself, so a fresh level never starts dirty.
        self.last_save_hash = BlankLevel::new().clean_hash();
        trace!("new_level_started");
    }

    /// Open the generation window, with the extent defaulting to the level's own.
    fn open_generate_dialog(&mut self) {
        self.generate_dialog.extent = self.resolved_level_size();
        // The remembered endpoint and model are loaded once at startup, not here: whatever
        // is in the dialog now is either that or something the user has since typed, and
        // reopening the window must not throw either away.
        self.generate_open = true;
        self.generate_message = None;
        trace!("generate_dialog_opened extent={:?}", self.generate_dialog.extent);
    }

    /// Everything the editor restores from its config on the first frame. One function so a
    /// test can pin the set: dropping a line from here fails that test rather than quietly
    /// making something stop being remembered, which is exactly how the endpoint and model
    /// came to be written and never read.
    fn initial_load(&mut self) {
        self.load_last_background_path();
        self.load_last_level_path();
        self.load_remembered_model_settings();
    }

    /// Take the remembered endpoint, model and seed from the config into the dialog. Called
    /// once at startup, beside the other remembered values, so a user who points the endpoint
    /// at a local model server finds it still there next time.
    fn load_remembered_model_settings(&mut self) {
        let settings = self.load_ai_settings();
        trace!("loaded_remembered_model_settings endpoint={}", settings.endpoint);
        // Through `adopt_settings`, so the seed reaches the box it is read from as well as
        // the struct.
        self.generate_dialog.adopt_settings(settings);
    }

    /// The model settings as the config file has them.
    fn load_ai_settings(&self) -> crate::ai_client::ModelSettings {
        let path = self.config_file_path();
        let config = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        generate_ui::read_settings(&config)
    }

    /// Remember the endpoint, model and seed, leaving the rest of the config alone.
    fn save_ai_settings(&self, settings: &crate::ai_client::ModelSettings) {
        let path = self.config_file_path();
        let mut config = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        generate_ui::write_settings(&mut config, settings);
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&path, json);
            trace!("saved_ai_settings endpoint={}", settings.endpoint);
        }
    }

    /// Put a generated level on the canvas, replacing what was open.
    ///
    /// One undoable step: `save_state` records the whole level first, so a single Ctrl+Z
    /// puts back the entities, the size and the spawn exactly as they were.
    fn apply_generated_level(&mut self, level: &LevelData) {
        self.save_state();
        self.entities = level
            .entities
            .iter()
            .map(|e| Entity::from_level_entity(e, self.toolbox_layout.as_ref()))
            .collect();
        self.level_size = level
            .level_size
            .map(|s| Vec2::new(s[0], s[1]))
            .and_then(level_size::validate);
        self.level_spawn = level.spawn.map(|p| Pos2::new(p[0], p[1]));
        self.level_exit = level.exit.map(|p| Pos2::new(p[0], p[1]));
        // A generated level has never been saved anywhere, so the next Save asks where to
        // put it rather than overwriting whatever was open before.
        self.last_level_path = None;
        self.selected_entity = None;
        self.editing_polygon_entity = None;
        self.editing_polygon_point = None;
        self.drawing_polygon = None;
        self.drawing_rope = None;
        // Look at the start of the level that was just made.
        if let Some(spawn) = self.level_spawn {
            let canvas = self.canvas_rect.map(|r| r.size()).unwrap_or(Vec2::new(1200.0, 800.0));
            let offset = Vec2::new(
                (spawn.x - canvas.x / 2.0).max(0.0),
                (spawn.y - canvas.y / 2.0).max(0.0),
            );
            self.scroll = ScrollModel { offset, velocity: Vec2::ZERO };
            self.scroll_offset = self.scroll.pixel_offset();
        }
        trace!(
            "generated_level_applied entities={} spawn={:?} exit={:?}",
            self.entities.len(),
            self.level_spawn,
            self.level_exit
        );
    }

    /// Run the command that was waiting on the unsaved-changes confirmation.
    fn run_pending_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::Exit => std::process::exit(0),
            PendingAction::NewLevel => self.new_level(),
            PendingAction::GenerateLevel => self.open_generate_dialog(),
        }
    }

    /// Start `action`, asking about unsaved work first when there is any.
    fn request_action(&mut self, action: PendingAction) {
        if self.has_unsaved_changes() {
            self.pending_action = Some(action);
        } else {
            self.run_pending_action(action);
        }
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

    /// Everything one undoable step restores.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            entities: self.entities.clone(),
            level_size: self.level_size,
            level_spawn: self.level_spawn,
            level_exit: self.level_exit,
            last_level_path: self.last_level_path.clone(),
        }
    }

    /// Put a snapshot back on the editor.
    fn restore(&mut self, snapshot: Snapshot) {
        self.entities = snapshot.entities;
        self.level_size = snapshot.level_size;
        self.level_spawn = snapshot.level_spawn;
        self.level_exit = snapshot.level_exit;
        self.last_level_path = snapshot.last_level_path;
    }

    fn save_state(&mut self) {
        // Save current state to undo stack
        self.undo_stack.push(self.snapshot());
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
            self.redo_stack.push(self.snapshot());
            // Restore previous state
            self.restore(previous_state);
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
            self.undo_stack.push(self.snapshot());
            // Restore next state
            self.restore(next_state);
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

    /// Returns `true` when a file was actually written; `false` when the user dismissed
    /// the destination dialog, so callers waiting on the save (the unsaved-changes
    /// confirmation) do not go ahead and throw the work away.
    fn save_level(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        trace!("save_level_start entities={}", self.entities.len());
        
        let level_data = LevelData {
            version: "1.0".to_string(),
            background: None, // TODO: Could store background path if needed
            background_size: if self.background_size != Vec2::ZERO {
                Some([self.background_size.x, self.background_size.y])
            } else {
                None
            },
            level_size: self.level_size.map(|s| [s.x, s.y]),
            spawn: self.level_spawn.map(|p| [p.x, p.y]),
            exit: self.level_exit.map(|p| [p.x, p.y]),
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
            return Ok(true);
        }

        trace!("save_level_cancelled");
        Ok(false)
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
        self.level_size = level_data
            .level_size
            .map(|s| Vec2::new(s[0], s[1]))
            .and_then(level_size::validate);
        self.level_spawn = level_data.spawn.map(|p| Pos2::new(p[0], p[1]));
        self.level_exit = level_data.exit.map(|p| Pos2::new(p[0], p[1]));
        
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

    /// True when `pos` is inside a floating panel (help window or Level Size dialog),
    /// whose clicks belong to that panel and must not reach the canvas beneath.
    /// True when `pos` is inside a floating panel (help window or Level Size dialog).
    /// The Level Size dialog is modal: while it is open nothing reaches the canvas. Its
    /// rect (and the help window's) is kept for one frame after closing so the very
    /// click that pressed OK / Cancel / the close button cannot fall through.
    fn pointer_over_panels(&self, pos: Pos2) -> bool {
        self.size_dialog.is_some()
            || [self.help_rect, self.size_dialog_rect]
                .iter()
                .flatten()
                .any(|rect| rect.contains(pos))
    }

    /// True when a click at `pos` belongs to egui UI rather than the level canvas: a
    /// floating panel, the menu bar, an open dropdown or window (anything painted on a
    /// non-background layer), or a menu item that was activated this frame. The last
    /// case matters because egui closes the dropdown in the same frame it reports the
    /// click, so the layer test alone would let that click through to the canvas.
    fn pointer_over_ui(&self, ctx: &egui::Context, pos: Pos2) -> bool {
        self.ui_consumed_click
            || self.pointer_over_panels(pos)
            || self.menu_bar_rect.is_some_and(|rect| rect.contains(pos))
            // The minimap is a mouse control of its own: a press on it never reaches the
            // canvas tools, and never moves the play spawn either. A gesture that began on
            // it stays consumed through its release frame, wherever the release lands —
            // egui still reports a click for a release a few pixels off the box.
            || self.minimap_drag
            || self.minimap_layout.is_some_and(|layout| layout.contains(pos))
            || ctx
                .layer_id_at(pos)
                .is_some_and(|layer| layer.order != egui::Order::Background)
    }

    /// Press-and-drag on the minimap re-centres the view on the pointed-at level spot.
    ///
    /// Runs after the scroll model has been stepped for the frame, so a drag re-applies
    /// its target on top of any arrow-key motion and always wins. While play mode runs
    /// the camera owns the offset and the minimap ignores the mouse (the press is still
    /// consumed by `pointer_over_ui`, so it cannot move the spawn point).
    fn handle_minimap_pointer(&mut self, ctx: &egui::Context, pointer: &egui::PointerState, panel: egui::Rect) {
        let Some(layout) = self.minimap_layout else {
            self.minimap_drag = false;
            return;
        };
        let pos = pointer.interact_pos();
        let hovering = pos.is_some_and(|p| layout.contains(p));

        // The gesture lasts from the press through the release frame (so the click egui
        // reports on release is still consumed, even a few pixels off the box), and ends
        // on the frame after. This bookkeeping runs in play mode too: the press is owned
        // by the minimap there as well, it just does nothing.
        if self.minimap_drag && !pointer.primary_down() && !pointer.primary_released() {
            self.minimap_drag = false;
        }
        if pointer.primary_pressed() && hovering {
            self.minimap_drag = true;
            // A click on an entity or vertex leaves a drag armed until the next release;
            // this press is the minimap's, so that drag must not follow the pointer.
            self.dragging_entity = false;
            self.dragging_polygon_point = false;
            self.drag_start = None;
            trace!("minimap_press {:?}", pos);
        }
        if self.play.is_some() {
            // The camera owns the view while playing: no jump, no drag, no hover cursor.
            return;
        }

        let dragging = self.minimap_drag && pointer.primary_down();
        if dragging {
            if let Some(p) = pos {
                // Instant: the offset is set directly and any momentum is discarded so
                // nothing glides on after release.
                self.scroll.offset = layout.offset_for_pointer(p, panel.size());
                self.scroll.velocity = Vec2::ZERO;
                self.scroll_offset = self.scroll.pixel_offset();
                trace!("minimap_drag pointer={:?} offset={:?}", p, self.scroll.offset);
            }
            ctx.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
            ctx.request_repaint();
        } else if hovering {
            ctx.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
        }
        if self.minimap_drag && pointer.primary_released() {
            trace!("minimap_release offset={:?}", self.scroll.offset);
        }
    }

    /// Maximum (x, y) reached by any entity, or `None` when the level is empty.
    fn entity_bounds(&self) -> Option<Vec2> {
        let mut max: Option<Vec2> = None;
        let mut grow = |x: f32, y: f32| {
            let m = max.get_or_insert(Vec2::ZERO);
            m.x = m.x.max(x);
            m.y = m.y.max(y);
        };
        for e in &self.entities {
            match e {
                Entity::Bitmap(b) => {
                    let size = b.get_size();
                    grow(b.pos.x + size.x, b.pos.y + size.y);
                }
                Entity::Polygon(poly) => {
                    for p in &poly.points {
                        grow(p.x, p.y);
                    }
                }
                Entity::Rope(rope) => {
                    let end = rope.end();
                    grow(end.x + game_config::ROPE_THICKNESS, end.y);
                }
            }
        }
        max
    }

    /// The level extent: explicit size, else background, else entity bounds, else default.
    fn resolved_level_size(&self) -> Vec2 {
        let bg = (self.background_size.x >= 1.0 && self.background_size.y >= 1.0)
            .then_some(self.background_size);
        level_size::resolve(self.level_size, bg, self.entity_bounds()).0
    }

    /// Collision geometry for play mode, mirroring the game's level import: every polygon
    /// is solid and `wall_tool` polygons are additionally climbable.
    fn collision_world(&self) -> World {
        let polys = self
            .entities
            .iter()
            .filter_map(|e| e.as_polygon())
            .filter_map(|poly| {
                let climbable = poly.polygon_type == game_config::CLIMBABLE_POLYGON_TYPE;
                CollisionPoly::new(&poly.points, climbable)
            })
            .collect();
        let ropes = self
            .entities
            .iter()
            .filter_map(|e| e.as_rope())
            .map(|r| sim::Rope { anchor: r.anchor, length: r.length })
            .collect();
        World { polys, ropes }
    }

    /// The level's bitmap entities in the plain form the combat layer maps to orcs,
    /// coins and death pits.
    fn bitmap_spawns(&self) -> Vec<BitmapSpawn> {
        self.entities
            .iter()
            .filter_map(|e| e.as_bitmap())
            .map(|b| BitmapSpawn {
                name: b.bitmap_name.clone(),
                pos: b.pos,
                size: b.get_size(),
            })
            .collect()
    }

    /// The world point at the centre of what is on screen. `None` until a frame has
    /// been laid out and there is a canvas to take a centre from.
    fn view_centre(&self) -> Option<Pos2> {
        self.canvas_rect.map(|r| r.center() + self.scroll_offset)
    }

    fn start_play(&mut self) {
        // A level that carries its own start — one that was generated, and proved playable
        // from exactly there — begins there. Otherwise spawn at the centre of what is on
        // screen, so trying a spot is a matter of scrolling until you can see it and
        // pressing play: a click made earlier somewhere else in the level no longer
        // decides where the run begins. Before the first frame there is no canvas to
        // measure, so fall back to the last spawn.
        let spawn = self
            .level_spawn
            .or_else(|| self.view_centre())
            .or(self.play_spawn)
            .unwrap_or(Pos2::new(100.0, 100.0));
        self.play_spawn = Some(spawn);
        self.tool_before_play = Some((self.tool.clone(), self.current_tool_name.clone()));
        self.view_before_play = Some(camera::capture(&self.scroll));
        // Drop any in-flight interaction so its release cannot commit an edit mid-play.
        self.dragging_entity = false;
        self.dragging_polygon_point = false;
        self.dragging_toolbox = false;
        self.drag_start = None;
        self.drawing_rope = None;
        self.play = Some(PlaySession::new(spawn, &self.bitmap_spawns()));
        trace!("play_started spawn={:?}", spawn);
    }

    /// Move the spawn point to `spawn` (world space) and restart the run there. This is
    /// what a canvas click does while playing; the level itself is never touched.
    fn move_play_spawn(&mut self, spawn: Pos2, panel: egui::Rect) {
        self.play_spawn = Some(spawn);
        self.last_click_pos = Some(spawn);
        if let Some(play) = &mut self.play {
            play.respawn_at(spawn);
        }
        // The character just jumped, after this frame's camera update and before it is
        // drawn. Re-apply the containment bound now so a click near a canvas edge cannot
        // show it off screen for a frame.
        self.contain_play_camera(panel);
        trace!("play_spawn_moved {:?}", spawn);
    }

    fn stop_play(&mut self) {
        if let Some((tool, name)) = self.tool_before_play.take() {
            self.tool = tool;
            self.current_tool_name = name;
        }
        if let Some(view) = self.view_before_play.take() {
            camera::restore(&mut self.scroll, view);
            self.scroll_offset = self.scroll.pixel_offset();
        }
        self.play = None;
        trace!("play_stopped");
    }

    fn toggle_play(&mut self) {
        if self.play.is_some() {
            self.stop_play();
        } else {
            self.start_play();
        }
    }

    /// Read the character's buttons, using the game's default bindings.
    fn play_input(ctx: &egui::Context) -> PlayInput {
        if ctx.wants_keyboard_input() {
            return PlayInput::default();
        }
        let sim = ctx.input(|i| sim::Input {
            left: i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft),
            right: i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight),
            up: i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp),
            down: i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown),
            jump: i.key_down(egui::Key::Space) || i.key_down(egui::Key::Z),
            dash: i.modifiers.shift || i.key_down(egui::Key::K) || i.key_down(egui::Key::C),
        });
        // Attack and shield use the game's own bindings; left-click stays the editor's
        // spawn-placement gesture, so it never attacks.
        ctx.input(|i| PlayInput {
            sim,
            attack: i.key_down(egui::Key::J) || i.key_down(egui::Key::X),
            shield: i.key_down(egui::Key::L),
        })
    }

    /// Follow the character with the canvas, using the game's camera constants.
    /// Re-apply the containment bound without smoothing, for when the character has been
    /// moved after this frame's [`Self::follow_with_camera`] has already run.
    fn contain_play_camera(&mut self, panel: egui::Rect) {
        let Some(pos) = self.play.as_ref().map(|s| s.player.pos) else { return };
        self.scroll.offset = camera::contain(self.scroll.offset, pos, panel);
        self.scroll_offset = self.scroll.pixel_offset();
    }

    /// Follow the character and keep it on screen. `panel` is the canvas rectangle, not
    /// the window: the camera centres on it, and the containment bound is measured
    /// against it. The offset may go negative here — while play owns the camera the
    /// editor's origin clamp does not apply, so a character at negative world
    /// coordinates is still shown.
    fn follow_with_camera(&mut self, panel: egui::Rect, dt: f32) {
        let Some(session) = &self.play else { return };
        let play = &session.player;
        self.scroll.offset = camera::follow(self.scroll.offset, play.pos, play.vel, panel, dt);
        self.scroll.velocity = Vec2::ZERO;
    }

    /// Draw the character capsule and the status overlay.
    fn draw_play(&self, painter: &egui::Painter, offset: Vec2) {
        let Some(session) = &self.play else { return };
        let play = &session.player;
        let c = camera::screen_pos(play.pos, offset);
        let r = game_config::PLAYER_CAPSULE_RADIUS;
        let hh = game_config::PLAYER_CAPSULE_HALF_HEIGHT;
        let body = egui::Rect::from_min_max(
            egui::pos2(c.x - r, c.y - hh),
            egui::pos2(c.x + r, c.y + hh),
        );
        let fill = Color32::from_rgb(90, 190, 255);
        painter.rect_filled(body, 0.0, fill);
        painter.circle_filled(egui::pos2(c.x, c.y - hh), r, fill);
        painter.circle_filled(egui::pos2(c.x, c.y + hh), r, fill);
        painter.circle_stroke(
            egui::pos2(c.x, c.y - hh),
            r,
            egui::Stroke::new(2.0_f32, Color32::BLACK),
        );
        // Facing tick.
        painter.line_segment(
            [egui::pos2(c.x, c.y), egui::pos2(c.x + play.facing * r, c.y)],
            egui::Stroke::new(3.0_f32, Color32::BLACK),
        );

        let combat = &session.combat;

        // Death pits, then coins, then orcs: hazards behind pickups behind bodies.
        for pit in &combat.pits {
            painter.rect_filled(
                pit.rect.translate(-offset),
                0.0,
                Color32::from_rgba_unmultiplied(160, 40, 40, 90),
            );
        }
        for coin in &combat.coins {
            if coin.collected {
                continue;
            }
            let r = coin.rect.translate(-offset);
            painter.circle_filled(r.center(), r.width().min(r.height()) * 0.5, Color32::from_rgb(235, 190, 60));
        }
        for orc in &combat.orcs {
            if !orc.alive {
                continue;
            }
            let body = orc.aabb().translate(-offset);
            let tint = if orc.club_active() {
                // The game tints the orc while the club can bite; the same warning here.
                Color32::from_rgb(210, 70, 70)
            } else {
                Color32::from_rgb(110, 160, 100)
            };
            painter.rect_filled(body, 4.0, tint);
            painter.rect_stroke(body, 4.0, egui::Stroke::new(2.0_f32, Color32::BLACK));
            // Power bar above the orc.
            let bar = egui::Rect::from_min_size(
                egui::pos2(body.left(), body.top() - 10.0),
                egui::vec2(body.width(), 5.0),
            );
            painter.rect_filled(bar, 0.0, Color32::from_gray(70));
            painter.rect_filled(
                egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * orc.power_fraction(), bar.height())),
                0.0,
                Color32::from_rgb(220, 90, 90),
            );
        }

        if self.combat_debug {
            self.draw_combat_debug(painter, offset);
        }
    }

    /// F2 view: what can hit what. Drawn only on request.
    fn draw_combat_debug(&self, painter: &egui::Painter, offset: Vec2) {
        let Some(session) = &self.play else { return };
        let combat = &session.combat;

        if let Some(slash) = &combat.attack.slash {
            let box_ = slash.direction.hitbox(session.player.pos, slash.facing).translate(-offset);
            let colour = if slash.is_live() {
                Color32::from_rgb(255, 200, 40)
            } else {
                Color32::from_gray(150)
            };
            painter.rect_stroke(box_, 0.0, egui::Stroke::new(2.0_f32, colour));
        }

        for orc in &combat.orcs {
            if !orc.alive {
                continue;
            }
            let c = orc.pos - offset;
            painter.circle_stroke(
                c,
                game_config::ORC_ATTACK_RANGE_PX,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 120, 220)),
            );
            if orc.club_active() {
                let danger = egui::Rect::from_center_size(
                    c,
                    egui::vec2(
                        game_config::ORC_CLUB_HIT_MAX_SEPARATION_X_PX * 2.0,
                        game_config::ORC_CLUB_HIT_MAX_SEPARATION_Y_PX * 2.0,
                    ),
                );
                painter.rect_stroke(danger, 0.0, egui::Stroke::new(2.0_f32, Color32::from_rgb(230, 60, 60)));
            }
            painter.text(
                egui::pos2(c.x, c.y - 24.0),
                egui::Align2::CENTER_BOTTOM,
                orc.state.label(),
                egui::FontId::monospace(11.0),
                Color32::BLACK,
            );
        }
    }

    /// Modal Level Size dialog. Applies only a valid size.
    /// The generation window, and the progress of a run in flight.
    ///
    /// The run lives on a worker thread; this only reads what it has reported. While one is
    /// in flight the context is asked to repaint every frame, so the elapsed time ticks and
    /// the stage line keeps up even though nothing else is happening.
    fn render_generate_dialog(&mut self, ctx: &egui::Context) {
        // Take whatever the worker has said, and deal with a run that has finished.
        let mut finished = None;
        if let Some(run) = &mut self.generate_run {
            let running = run.poll();
            if running {
                ctx.request_repaint();
            } else {
                finished = self.generate_run.take().and_then(|r| r.outcome);
            }
        }
        if let Some(outcome) = finished {
            match outcome {
                Ok(generated) => {
                    // The window stays up so the report can be read — in particular
                    // whether the level is the model's own work or was repaired. The level
                    // itself is already on the canvas behind it.
                    self.generate_message = Some(generate_ui::report(&generated));
                    self.apply_generated_level(&generated.level);
                }
                Err(why) => {
                    // Nothing is applied, so the open level is exactly as it was.
                    self.generate_message = Some(why.to_string());
                    trace!("generate_failed reason={}", why);
                }
            }
        }

        if !self.generate_open {
            return;
        }

        let mut open = true;
        let mut close = false;
        let mut start = false;
        let mut cancel = false;
        let running = self.generate_run.is_some();
        let dialog = &mut self.generate_dialog;
        let message = self.generate_message.clone();
        let stage = self
            .generate_run
            .as_ref()
            .map(|r| (r.stage.clone(), r.started.elapsed().as_secs()));

        egui::Window::new("Generate Level with AI")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    ui.label("Describe the area you want:");
                    ui.add(
                        egui::TextEdit::multiline(&mut dialog.prompt)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .hint_text("a flooded cistern, heavy on wall-jumps, ending in a boss hall"),
                    );
                    ui.separator();
                    egui::Grid::new("generate_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Endpoint");
                        ui.text_edit_singleline(&mut dialog.settings.endpoint);
                        ui.end_row();
                        ui.label("Model");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.settings.model)
                                .hint_text("the model your local server has loaded"),
                        );
                        ui.end_row();
                        ui.label("Chambers");
                        ui.add(egui::Slider::new(
                            &mut dialog.chambers,
                            generate_ui::CHAMBERS_MIN..=generate_ui::CHAMBERS_MAX,
                        ));
                        ui.end_row();
                        ui.label("Seed");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.seed_text)
                                .hint_text("optional, for a repeatable level"),
                        );
                        ui.end_row();
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "Into {:.0} x {:.0} px. The model writes the level itself, and the \
                             editor will not hand it over until the play simulation has walked \
                             a route through it.",
                            dialog.extent.x, dialog.extent.y
                        ))
                        .weak(),
                    );
                    // The key situation, never the key.
                    let status = dialog.key_status();
                    let colour = match status {
                        generate_ui::KeyStatus::Missing => Color32::from_rgb(200, 120, 60),
                        _ => Color32::from_gray(140),
                    };
                    ui.label(egui::RichText::new(status.line()).color(colour));
                });

                ui.separator();
                if let Some((stage, secs)) = &stage {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(generate_ui::stage_line(stage, *secs));
                    });
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                } else {
                    ui.horizontal(|ui| {
                        let can = dialog.can_generate();
                        let why = dialog.why_not_generate().unwrap_or_default();
                        if ui
                            .add_enabled(can, egui::Button::new("Generate"))
                            .on_disabled_hover_text(why)
                            .clicked()
                        {
                            start = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }

                if let Some(message) = &message {
                    ui.separator();
                    ui.label(message);
                }
            });

        if cancel {
            // Tell the run to stop, then let go of it. The worker checks the flag between
            // requests, but it cannot be interrupted part-way through one, and a reply from
            // a slow server can be minutes away; holding on would leave the user watching a
            // spinner for a request whose answer is already unwanted. Dropping the handle
            // discards whatever it eventually produces, so the open level is safe either
            // way, and Cancel is immediate as far as the user is concerned.
            if let Some(run) = self.generate_run.take() {
                run.cancel();
            }
            self.generate_message =
                Some("Generation cancelled. Your level has not been touched.".to_string());
            trace!("generate_cancelled");
        }
        if start {
            let settings = self.generate_dialog.model_settings();
            self.save_ai_settings(&settings);
            let params = self.generate_dialog.params();
            self.generate_message = None;
            self.generate_run =
                Some(GenerateRun::start(self.generate_dialog.client(), params));
            trace!("generate_started chambers={} extent={:?}", self.generate_dialog.chambers, self.generate_dialog.extent);
        }
        if (close || !open) && self.generate_run.is_none() {
            self.generate_open = false;
        }
    }

    fn render_size_dialog(&mut self, ctx: &egui::Context) {
        let Some((w_text, h_text)) = self.size_dialog.clone() else {
            self.size_dialog_rect = None;
            return;
        };
        let mut w_text = w_text;
        let mut h_text = h_text;
        let mut close = false;
        let mut open = true;
        let mut error: Option<&str> = None;

        let response = egui::Window::new("Level Size")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("level_size_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Width");
                    ui.text_edit_singleline(&mut w_text);
                    ui.end_row();
                    ui.label("Height");
                    ui.text_edit_singleline(&mut h_text);
                    ui.end_row();
                });
                ui.label(
                    egui::RichText::new(format!(
                        "{} to {} px per side",
                        level_size::MIN_LEVEL_EDGE,
                        level_size::MAX_LEVEL_EDGE
                    ))
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        let parsed = (w_text.trim().parse::<f32>(), h_text.trim().parse::<f32>());
                        match parsed {
                            (Ok(w), Ok(h)) => match level_size::validate(Vec2::new(w, h)) {
                                Some(size) => {
                                    self.level_size = Some(size);
                                    trace!("level_size_set {:?}", size);
                                    close = true;
                                }
                                None => error = Some("Size out of range; unchanged."),
                            },
                            _ => error = Some("Width and height must be numbers."),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                if let Some(msg) = error {
                    ui.colored_label(Color32::from_rgb(200, 60, 60), msg);
                    trace!("level_size_rejected w={:?} h={:?}", w_text, h_text);
                }
            });

        // Keep the rect through the closing frame; the early return above clears it
        // on the next frame once the dialog is gone.
        self.size_dialog_rect = response.map(|r| r.response.rect);
        if close || !open {
            self.size_dialog = None;
        } else {
            self.size_dialog = Some((w_text, h_text));
        }
    }

    /// Draw the Keyboard & Commands window. Non-modal: the canvas, toolbox and every
    /// shortcut keep working while it is open.
    fn render_help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            self.help_rect = None;
            return;
        }

        let undo = ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Z,
        ));
        let redo = ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        ));
        let content = HelpContent::build(
            self.toolbox_layout.as_ref().map(|l| l.tools.as_slice()),
            &undo,
            &redo,
        );

        // Keep the whole window on screen at the 800x720 minimum size.
        let max_body = (ctx.screen_rect().height() * 0.7).max(240.0);
        let mut open = true;
        let response = egui::Window::new("Keyboard & Commands")
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_body)
                    .show(ui, |ui| {
                        let sections = [
                            ("Keyboard", &content.keyboard),
                            ("Mouse", &content.mouse),
                            ("Tools", &content.tools),
                            ("Menus", &content.file),
                        ];
                        for (title, entries) in sections {
                            ui.heading(title);
                            egui::Grid::new(format!("help_grid_{}", title))
                                .num_columns(2)
                                .striped(true)
                                .spacing([16.0, 4.0])
                                .show(ui, |ui| {
                                    for entry in entries {
                                        ui.label(egui::RichText::new(&entry.label).strong());
                                        ui.label(&entry.effect);
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(8.0);
                        }
                    });
            });

        self.help_rect = response.map(|r| r.response.rect);

        if !open {
            // Leave help_rect set for this frame so the close-button click is absorbed;
            // the early return above clears it next frame.
            self.help_open = false;
            trace!("help_closed_by_button");
        }
    }
}

/// Draw a bottom-right minimap: full-level thumbnail when a background is loaded, and a rectangle for the visible area.
///
/// `layout` is the frame's shared minimap geometry (see `minimap::layout`), so the box
/// drawn here is exactly the box the mouse hit-test uses.
fn render_level_minimap(
    ctx: &egui::Context,
    bg: &mut BackgroundImageController,
    layout: Option<MinimapLayout>,
    scroll_offset: Vec2,
    panel: egui::Rect,
) {
    let timestamp_secs = ctx.input(|i| i.time);
    let pixels_per_point = ctx.input(|i| i.pixels_per_point());
    let screen = ctx.screen_rect();
    let Some(MinimapLayout { map_rect, inner, level }) = layout else {
        write_minimap_debug_json(&MinimapDebugSnapshot::early_only(
            timestamp_secs,
            panel,
            "panel_clip_too_small",
            screen,
            pixels_per_point,
        ));
        return;
    };
    let map_w = map_rect.width();
    let map_h = map_rect.height();

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
            self.initial_load();
        }

        trace!("update_menu_bar_start");
        self.ui_consumed_click = false;
        // Top menu bar
        let menu_bar = egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let can_edit = self.play.is_none();
                    if ui
                        .add_enabled(can_edit, egui::Button::new("New Level"))
                        .on_disabled_hover_text("Stop play mode first")
                        .clicked()
                    {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        trace!("menu_new_level_clicked");
                        self.request_action(PendingAction::NewLevel);
                    }
                    if ui.button("Background Image").clicked() {
                        self.ui_consumed_click = true;
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
                    if ui
                        .add_enabled(can_edit, egui::Button::new("Load Level"))
                        .on_disabled_hover_text("Stop play mode first")
                        .clicked()
                    {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        trace!("menu_load_clicked");
                        if let Err(e) = self.load_level() {
                            error!("load_level_error error={}", e);
                        }
                    }
                    if ui.button("Save Level").clicked() {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        trace!("menu_save_clicked");
                        if let Err(e) = self.save_level() {
                            error!("save_level_error error={}", e);
                        }
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        self.request_action(PendingAction::Exit);
                    }
                });
                ui.menu_button("Level", |ui| {
                    // Generating replaces the level being played, exactly as New Level and
                    // Load Level would, so it is unavailable for the same reason.
                    let can_edit = self.play.is_none();
                    if ui
                        .add_enabled(can_edit, egui::Button::new("Generate Level with AI\u{2026}"))
                        .on_disabled_hover_text("Stop play mode first")
                        .clicked()
                    {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        trace!("menu_generate_level_clicked");
                        self.request_action(PendingAction::GenerateLevel);
                    }
                    if ui.button("Level Size\u{2026}").clicked() {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        let size = self.resolved_level_size();
                        self.size_dialog = Some((size.x.to_string(), size.y.to_string()));
                        trace!("menu_level_size_clicked current={:?}", size);
                    }
                });
                ui.menu_button("Play", |ui| {
                    let label = if self.play.is_some() { "Stop" } else { "Play" };
                    if ui.button(label).clicked() {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        self.toggle_play();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard & Commands").clicked() {
                        self.ui_consumed_click = true;
                        ui.close_menu();
                        self.help_open = true;
                        trace!("menu_help_clicked");
                    }
                });
            });
        });
        self.menu_bar_rect = Some(menu_bar.response.rect);

        self.render_help_window(ctx);
        self.render_size_dialog(ctx);
        self.render_generate_dialog(ctx);
        
        // Handle exit with unsaved changes prompt
        if let Some(action) = self.pending_action {
            let mut open = true;
            let mut resolved = false;
            egui::Window::new("Unsaved Changes")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(action.prompt());
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            // Dismissing the destination dialog, or a failed write, leaves
                            // the work unsaved: keep the confirmation open rather than
                            // discarding it behind the user's back.
                            match self.save_level() {
                                Ok(saved) => resolved = saved,
                                Err(e) => error!("save_level_error error={}", e),
                            }
                        }
                        if ui.button("Discard").clicked() {
                            resolved = true;
                        }
                        if ui.button("Cancel").clicked() {
                            // Abandon the command entirely; nothing is changed.
                            self.pending_action = None;
                        }
                    });
                });
            // Closing the window with its X is a cancel.
            if !open {
                self.pending_action = None;
            }
            if resolved {
                self.pending_action = None;
                self.run_pending_action(action);
            }
        }

        trace!("update_menu_bar_end");

        trace!("update_handle_input_start");
        // Two-axis arrow-key scrolling with momentum. Arrow keys are ignored while a
        // text field owns the keyboard so text entry keeps working.
        let dt = ctx.input(|i| i.stable_dt);
        let held = if ctx.wants_keyboard_input() || self.play.is_some() {
            // While playing, the arrow keys drive the character instead of the canvas.
            HeldDirs::default()
        } else {
            ctx.input(|i| HeldDirs {
                left: i.key_down(egui::Key::ArrowLeft),
                right: i.key_down(egui::Key::ArrowRight),
                up: i.key_down(egui::Key::ArrowUp),
                down: i.key_down(egui::Key::ArrowDown),
            })
        };
        if !ctx.wants_keyboard_input() && self.size_dialog.is_none() {
            let toggle_help = ctx.input(|i| {
                i.key_pressed(egui::Key::F1) || i.key_pressed(egui::Key::Questionmark)
            });
            if toggle_help {
                self.help_open = !self.help_open;
                trace!("help_toggled open={}", self.help_open);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
                self.toggle_play();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                self.combat_debug = !self.combat_debug;
                trace!("combat_debug_toggled on={}", self.combat_debug);
            }
            if self.play.is_some() && ctx.input(|i| i.key_pressed(egui::Key::R)) {
                if let Some(play) = &mut self.play {
                    play.restart();
                    trace!("play_respawned");
                }
            }
        }

        // Advance the play simulation on its own fixed timestep, then let the camera
        // follow. Play mode always repaints so the motion is continuous.
        // The canvas rectangle, taken once so the frame's follow and any later re-bind
        // measure the character against exactly the same area.
        let play_panel = ctx.available_rect();
        if self.play.is_some() {
            let input = Self::play_input(ctx);
            let world = self.collision_world();
            if let Some(play) = &mut self.play {
                play.advance(&world, input, dt);
            }
            self.follow_with_camera(play_panel, dt);
            ctx.request_repaint();
        }

        // While play owns the camera the editor's scroll model is not stepped: its
        // origin clamp would otherwise pull the camera back every frame and stop it
        // showing anything left of or above the level origin.
        if self.play.is_none() {
            // The reachable area is the level plus one viewport of slack: an offset of
            // level_size puts the level's far edge at the viewport's near edge.
            self.scroll.step(held, dt, self.resolved_level_size());
        }
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
            
            // Every editor command below can change the level, so all of them are inert
            // while play mode is running.
            let editing = self.play.is_none() && self.size_dialog.is_none();

            // Finalize polygon with Enter key
            if editing && i.key_pressed(egui::Key::Enter) {
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
            if editing && i.key_pressed(egui::Key::Z) && i.modifiers.command {
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
                // Closing the help always wins, so a stray Escape can never also
                // discard a half-drawn polygon.
                let action = if escape_closes_help(self.help_open) {
                    EscapeAction::CloseHelp
                } else {
                    escape_action(
                        self.help_open,
                        editing && self.drawing_polygon.is_some(),
                        editing && self.editing_polygon_entity.is_some(),
                        editing && self.drawing_rope.is_some(),
                    )
                };
                match action {
                    EscapeAction::CloseHelp => {
                        self.help_open = false;
                        trace!("help_closed_by_escape");
                    }
                    EscapeAction::CancelPolygon => {
                        self.drawing_polygon = None;
                        trace!("polygon_cancelled");
                    }
                    EscapeAction::CancelRope => {
                        self.drawing_rope = None;
                        trace!("rope_cancelled");
                    }
                    EscapeAction::ExitEditMode => {
                        self.editing_polygon_entity = None;
                        self.editing_polygon_point = None;
                        self.dragging_polygon_point = false;
                        self.selected_entity = None;
                        trace!("polygon_edit_mode_cancelled");
                    }
                    EscapeAction::Nothing => {}
                }
            }
            
            // Delete selected polygon point with Delete or Backspace key
            if editing && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
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

        // Minimap (navigation box) mouse navigation. The layout is computed once here,
        // before any click is dispatched, from the same canvas rect the renderer uses.
        let canvas_panel = ctx.available_rect();
        // Remembered for the next frame's play start, which runs before this point.
        self.canvas_rect = Some(canvas_panel);
        self.minimap_layout = minimap::layout(canvas_panel, ctx.screen_rect(), self.resolved_level_size());
        self.handle_minimap_pointer(ctx, &pointer, canvas_panel);

        trace!("update_toolbox_start toolbox_pos={:?} dragging={}", self.toolbox_pos, self.dragging_toolbox);
        // Render toolbox and get tool selection
        let mut tool_selected = None;
        if let Some(layout) = &self.toolbox_layout {
            tool_selected = render_toolbox(ctx, layout, self.toolbox_pos, &mut self.loaded_textures, &self.current_tool_name);
        }
        trace!("update_toolbox_end tool_selected={:?}", tool_selected);
        
        trace!("update_handle_toolbox_drag_start");
        // A press inside the help window belongs to the window, not the toolbox. This
        // only blocks *starting* a drag: one already under way keeps tracking the
        // cursor across the window, so the toolbox never jumps.
        let pointer_over_help = pointer_pos.is_some_and(|pos| self.pointer_over_panels(pos));
        // Drag logic for toolbox
        if pointer_pressed && self.toolbox_layout.is_some() {
            let layout = self.toolbox_layout.as_ref().unwrap();
            let toolbox_rect = get_toolbox_rect(layout, self.toolbox_pos);
            
            if let Some(pos) = pointer_pos {
                if toolbox_rect.contains(pos) && !pointer_over_help {
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
            // Switching tools abandons a rope whose bottom end was never clicked.
            self.drawing_rope = None;
            
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
                Some("rope") => Tool::Rope,
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
        if self.dragging_polygon_point && pointer_pressed && self.play.is_none() {
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
        if self.dragging_entity && pointer_pressed && self.play.is_none() {
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

            // Level boundary: the editable extent, scrolling with everything else.
            let bounds = level_size::boundary_rect(self.resolved_level_size())
                .translate(-self.scroll_offset);
            painter.rect_stroke(
                bounds,
                0.0,
                egui::Stroke::new(1.5_f32, Color32::from_rgb(120, 140, 200)),
            );

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

            // Rope being placed: a translucent preview from the anchor down to the cursor.
            if let (Tool::Rope, Some(anchor), Some(mouse)) = (&self.tool, self.drawing_rope, pointer_pos) {
                let a = anchor - self.scroll_offset;
                let end_y = mouse.y.max(a.y + game_config::ROPE_MIN_LENGTH);
                let ghost = Color32::from_rgba_unmultiplied(ROPE_COLOR.r(), ROPE_COLOR.g(), ROPE_COLOR.b(), 128);
                painter.line_segment([a, egui::pos2(a.x, end_y)], (game_config::ROPE_THICKNESS, ghost));
                painter.circle_filled(a, game_config::ROPE_THICKNESS, ROPE_COLOR);
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
            let mut rope_idx = 0;
            for (idx, entity) in self.entities.iter().enumerate() {
                let is_selected = self.selected_entity == Some(idx);
                let is_editing = self.editing_polygon_entity == Some(idx);

                // Ropes swing while playing: draw them along their current angle. The
                // simulation's rope list is built from the entities in this same order.
                if let Entity::Rope(rope) = entity {
                    match &self.play {
                        Some(play) => {
                            // The flexible chain, with the hand point inserted while held.
                            let line: Vec<Pos2> = play
                                .player
                                .rope_polyline(rope_idx)
                                .into_iter()
                                .map(|p| p - self.scroll_offset)
                                .collect();
                            if line.is_empty() {
                                entity.draw(painter, self.scroll_offset, is_selected);
                            } else {
                                RopeEntity::paint_polyline(painter, &line, false);
                            }
                            let _ = rope;
                        }
                        None => entity.draw(painter, self.scroll_offset, is_selected),
                    }
                    rope_idx += 1;
                    continue;
                }
                
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
                    // Clicks inside a floating panel are consumed by that panel, so they
                    // never place, select or delete anything on the canvas behind it.
                    let help_click = self.pointer_over_ui(ctx, pos);

                    // While playing, a canvas click only moves the spawn point; the level
                    // is never touched.
                    if self.play.is_some() && !toolbox_click && !help_click {
                        self.move_play_spawn(pos + self.scroll_offset, play_panel);
                    } else if !toolbox_click && !help_click {
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
                            Tool::Rope => {
                                match RopeEntity::place(self.drawing_rope, world_pos) {
                                    RopePlacement::SetAnchor(anchor) => {
                                        self.drawing_rope = Some(anchor);
                                        trace!("rope_anchor_set {:?}", anchor);
                                    }
                                    RopePlacement::Finish { anchor, length } => {
                                        self.save_state();
                                        self.entities.push(Entity::new_rope(anchor, length));
                                        self.drawing_rope = None;
                                        trace!("rope_placed anchor={:?} length={} total={}", anchor, length, self.entities.len());
                                    }
                                    RopePlacement::Ignore => {
                                        trace!("rope_click_ignored {:?}", world_pos);
                                    }
                                }
                                self.last_click_time = Some(current_time);
                                self.last_click_pos = Some(world_pos);
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
            let secondary_over_help = pointer_pos.is_some_and(|pos| self.pointer_over_ui(ctx, pos));
            // Right-click finalises a polygon, so it must not fire while playing.
            if secondary_clicked
                && self.tool == Tool::DrawPolygon
                && !secondary_over_help
                && self.play.is_none()
            {
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

            self.draw_play(painter, self.scroll_offset);
            if let Some(session) = &self.play {
                let play = &session.player;
                let c = &session.combat;
                // Each shield state must read differently: a bare duration cannot say
                // whether it is protecting or locked out.
                let shield = if c.shield.is_active() {
                    format!("shield UP {:.1}s", c.shield.active_left())
                } else if c.shield.is_ready() {
                    "shield ready".to_string()
                } else {
                    format!("shield cooling {:.1}s", c.shield.cooldown_left())
                };
                let text = format!(
                    concat!(
                        "PLAY  pos ({:.0}, {:.0})  {}  {}\n",
                        "health {}/{}  score {}  coins {}/{}  orcs {}/{}  {}\n",
                        "[F5 stop] [R restart] [J/X attack] [L shield] [F2 debug] [click sets spawn]",
                    ),
                    play.pos.x,
                    play.pos.y,
                    if play.grounded { "grounded" } else { "airborne" },
                    play.state.label(),
                    c.health.max(0),
                    game_config::INITIAL_HEALTH,
                    c.score,
                    c.coins_collected(),
                    c.coins.len(),
                    c.orcs_alive(),
                    c.orcs.len(),
                    shield,
                );
                let panel = ui.clip_rect();
                painter.text(
                    egui::pos2(panel.left() + 8.0, panel.top() + 8.0),
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::monospace(13.0),
                    Color32::from_rgb(30, 30, 30),
                );
            }

            render_level_minimap(
                ctx,
                &mut self.background_controller,
                self.minimap_layout,
                self.scroll_offset,
                canvas_panel,
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

#[cfg(test)]
mod editor_state_tests {
    use super::*;

    fn a_polygon() -> Entity {
        Entity::new_polygon_with_type_and_color(
            vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0)],
            "wall_tool".to_string(),
            None,
        )
    }

    /// An editor with a level open in it and every piece of level-bearing state dirtied,
    /// so `new_level` has something to clear on each of them.
    fn a_working_editor() -> EditorState {
        let mut e = EditorState::default();
        e.entities = vec![a_polygon(), a_polygon()];
        e.level_size = Some(Vec2::new(4000.0, 3000.0));
        e.background_size = Vec2::new(4000.0, 3000.0);
        e.background_controller
            .load_image(&std::path::PathBuf::from("src/assets/wall.png"))
            .expect("the bundled wall.png should load");
        e.play_spawn = Some(Pos2::new(120.0, 340.0));
        e.last_click_pos = Some(Pos2::new(500.0, 600.0));
        e.selected_entity = Some(1);
        e.editing_polygon_entity = Some(1);
        e.editing_polygon_point = Some(2);
        e.dragging_entity = true;
        e.dragging_polygon_point = true;
        e.drawing_polygon = Some(vec![Pos2::new(0.0, 0.0)]);
        e.undo_stack = vec![Snapshot { entities: vec![a_polygon()], ..Snapshot::default() }];
        e.redo_stack = vec![Snapshot::default()];
        e.scroll = ScrollModel { offset: Vec2::new(900.0, 700.0), velocity: Vec2::new(400.0, -200.0) };
        e.scroll_offset = e.scroll.pixel_offset();
        e.last_level_path = Some(std::path::PathBuf::from("/tmp/some_level.json"));
        e.last_background_path = Some(std::path::PathBuf::from("src/assets/wall.png"));
        e.size_dialog = Some(("4000".to_string(), "3000".to_string()));
        e.last_save_hash = e.compute_entities_hash();
        e
    }

    // ── AI level generation ──────────────────────────────────────────────────

    /// A small generated level: floor, a start and an end.
    fn a_generated_level() -> LevelData {
        LevelData {
            version: "1.0".to_string(),
            background: None,
            background_size: None,
            level_size: Some([5000.0, 2500.0]),
            spawn: Some([1200.0, 700.0]),
            exit: Some([4200.0, 700.0]),
            entities: vec![
                level_data::LevelEntity::Polygon {
                    vertices: vec![[0.0, 900.0], [5000.0, 900.0], [5000.0, 1000.0], [0.0, 1000.0]],
                    polygon_type: Some("blocker_tool".to_string()),
                    color: None,
                },
                level_data::LevelEntity::Bitmap {
                    position: [2000.0, 800.0],
                    bitmap_name: "coin_tool".to_string(),
                    size: [64.0, 64.0],
                },
            ],
        }
    }

    // A13 — one undoable step restores the entities, the size and the spawn.
    #[test]
    fn a_generated_level_replaces_the_open_one_in_a_single_undoable_step() {
        let mut e = a_working_editor();
        e.level_spawn = Some(Pos2::new(11.0, 22.0));
        e.level_exit = Some(Pos2::new(33.0, 44.0));
        let before_entities = e.entities.clone();
        let before_size = e.level_size;
        let before_spawn = e.level_spawn;
        let before_exit = e.level_exit;
        let undo_depth = e.undo_stack.len();

        e.apply_generated_level(&a_generated_level());

        assert_eq!(e.entities.len(), 2, "the generated level is on the canvas");
        assert_eq!(e.level_size, Some(Vec2::new(5000.0, 2500.0)));
        assert_eq!(e.level_spawn, Some(Pos2::new(1200.0, 700.0)));
        assert_eq!(e.level_exit, Some(Pos2::new(4200.0, 700.0)));
        assert_eq!(
            e.undo_stack.len(),
            undo_depth + 1,
            "exactly one step was pushed, so one Ctrl+Z is enough"
        );

        e.undo();

        assert_eq!(
            format!("{:?}", e.entities),
            format!("{before_entities:?}"),
            "undo puts the entities back"
        );
        assert_eq!(e.level_size, before_size, "and the level size");
        assert_eq!(e.level_spawn, before_spawn, "and the spawn");
        assert_eq!(e.level_exit, before_exit, "and the exit");
    }

    #[test]
    fn undoing_a_generated_level_gives_back_the_file_the_old_one_came_from() {
        let mut e = a_working_editor();
        let path = std::path::PathBuf::from("/tmp/some_level.json");
        e.last_level_path = Some(path.clone());

        e.apply_generated_level(&a_generated_level());
        assert_eq!(e.last_level_path, None, "a generated level has no file of its own");

        e.undo();

        assert_eq!(
            e.last_level_path,
            Some(path),
            "undo must give back the level *and* the file it came from, so the next save \
             overwrites it rather than asking where to put it"
        );
    }

    #[test]
    fn a_generated_level_forgets_the_path_of_the_level_it_replaced() {
        let mut e = a_working_editor();
        assert!(e.last_level_path.is_some(), "the fixture has a level open");
        e.apply_generated_level(&a_generated_level());
        assert_eq!(
            e.last_level_path, None,
            "saving a generated level must ask where to put it, not overwrite what was open"
        );
    }

    #[test]
    fn a_generated_level_clears_any_selection_or_half_drawn_shape() {
        let mut e = a_working_editor();
        e.apply_generated_level(&a_generated_level());
        assert_eq!(e.selected_entity, None);
        assert_eq!(e.editing_polygon_entity, None);
        assert_eq!(e.editing_polygon_point, None);
        assert_eq!(e.drawing_polygon, None);
    }

    /// An editor whose config is a file of this test's own, in the scratch directory.
    fn an_editor_with_its_own_config(name: &str) -> (EditorState, std::path::PathBuf) {
        // The process id keeps concurrent `cargo test` runs on one machine from colliding.
        let path = std::env::temp_dir()
            .join(format!("level_editor_test_config_{name}_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut e = EditorState::default();
        e.config_path_override = Some(path.clone());
        (e, path)
    }

    // A2, second half — the endpoint and model the editor remembers really do come back.
    //
    // This is the test that was missing: the settings were being written on every Generate
    // and never read, because the only call that loaded them sat behind a condition that a
    // prefilled default model made permanently false. Round-tripping the two pure functions
    // did not notice, because the editor never called them.
    #[test]
    fn the_endpoint_and_model_come_back_after_a_restart() {
        let (mut first, path) = an_editor_with_its_own_config("restart");

        // The user points the editor at a model on their own machine and generates.
        first.generate_dialog.settings = crate::ai_client::ModelSettings {
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "llama3.1:8b".to_string(),
            seed: Some(4242),
        };
        first.save_ai_settings(&first.generate_dialog.settings.clone());

        // A fresh editor, as if restarted, reading the same config.
        let mut second = EditorState::default();
        second.config_path_override = Some(path.clone());
        assert_eq!(
            second.generate_dialog.settings.endpoint,
            crate::ai_client::DEFAULT_ENDPOINT,
            "before loading, a fresh editor holds the defaults"
        );

        second.load_remembered_model_settings();

        assert_eq!(
            second.generate_dialog.settings.endpoint, "http://localhost:11434/v1",
            "the endpoint the user chose must survive a restart, or the local-model \
             workflow silently reverts to OpenAI on every launch"
        );
        assert_eq!(second.generate_dialog.settings.model, "llama3.1:8b");
        assert_eq!(second.generate_dialog.settings.seed, Some(4242));
        assert_eq!(
            second.generate_dialog.seed_text, "4242",
            "the seed must reach the box it is read from, or the next Generate sends none \
             and then erases the remembered one"
        );
        assert_eq!(
            second.generate_dialog.model_settings().seed,
            Some(4242),
            "and a request made now must actually carry it"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// Everything the first frame restores. This pins the set: if a line is dropped from
    /// `initial_load`, whatever it restored stops being restored and this fails.
    #[test]
    fn the_first_frame_restores_everything_the_editor_remembers() {
        let (mut e, path) = an_editor_with_its_own_config("initial_load");
        e.save_level_path(&std::path::PathBuf::from("/tmp/no_such_level.json"));
        e.save_ai_settings(&crate::ai_client::ModelSettings {
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "a-model".to_string(),
            seed: Some(7),
        });

        let mut fresh = EditorState::default();
        fresh.config_path_override = Some(path.clone());
        fresh.initial_load();

        // The model settings, which is what regressed before.
        assert_eq!(fresh.generate_dialog.settings.endpoint, "http://localhost:11434/v1");
        assert_eq!(fresh.generate_dialog.settings.model, "a-model");
        assert_eq!(fresh.generate_dialog.seed_text, "7");
        // The remembered level path is consulted too. The file does not exist, so nothing is
        // loaded from it and `last_level_path` stays unset — the point here is that
        // `initial_load` reads the config at all rather than what it does with a missing file.
        assert_eq!(fresh.last_level_path, None, "a level that is gone is not loaded");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_fresh_config_leaves_the_openai_defaults_in_place() {
        let (mut e, path) = an_editor_with_its_own_config("fresh");
        e.load_remembered_model_settings();
        assert_eq!(e.generate_dialog.settings.endpoint, "https://api.openai.com/v1");
        assert_eq!(e.generate_dialog.settings.model, "gpt-6-astra");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remembering_the_model_settings_does_not_disturb_the_remembered_level() {
        let (mut e, path) = an_editor_with_its_own_config("coexist");
        let level = std::path::PathBuf::from("/tmp/some_level.json");
        e.save_level_path(&level);
        e.save_ai_settings(&crate::ai_client::ModelSettings {
            endpoint: "http://localhost:1234/v1".to_string(),
            model: "a-model".to_string(),
            seed: None,
        });

        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("written")).expect("json");
        assert_eq!(config["last_level"], "/tmp/some_level.json", "the level path survives");
        assert_eq!(config["ai_endpoint"], "http://localhost:1234/v1");
        assert_eq!(config["ai_model"], "a-model");

        // A21 — and no key is anywhere in what was written.
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(!text.contains("OPENAI_API_KEY"), "got: {text}");
        assert!(!text.to_lowercase().contains("api_key"), "no key field exists at all: {text}");
        if let Some(key) = crate::ai_client::api_key_from_env() {
            assert!(!text.contains(&key), "the key reached the config file");
        }

        let _ = std::fs::remove_file(&path);
    }

    // A14 — generation goes through the existing unsaved-changes confirmation.
    #[test]
    fn generating_with_unsaved_changes_asks_first_and_cancel_changes_nothing() {
        let mut e = a_working_editor();
        e.entities.push(a_polygon()); // Now dirty.
        assert!(e.has_unsaved_changes());
        let before = e.entities.len();

        e.request_action(PendingAction::GenerateLevel);

        assert_eq!(
            e.pending_action,
            Some(PendingAction::GenerateLevel),
            "it must ask before destroying unsaved work"
        );
        assert!(!e.generate_open, "and must not open the window until that is settled");
        assert_eq!(e.entities.len(), before, "nothing was touched");

        // Cancel is the dialog simply dropping the pending action.
        e.pending_action = None;
        assert_eq!(e.entities.len(), before, "cancelling leaves the level alone");
        assert!(!e.generate_open);
    }

    #[test]
    fn generating_with_no_unsaved_changes_opens_the_window_straight_away() {
        let mut e = a_working_editor();
        e.last_save_hash = e.compute_entities_hash(); // Clean.
        assert!(!e.has_unsaved_changes());

        e.request_action(PendingAction::GenerateLevel);

        assert_eq!(e.pending_action, None, "there is nothing to ask about");
        assert!(e.generate_open, "so the window opens at once");
    }

    #[test]
    fn the_generate_confirmation_says_what_it_is_about_to_do() {
        let prompt = PendingAction::GenerateLevel.prompt();
        assert!(prompt.contains("unsaved changes"), "{prompt}");
        assert!(prompt.contains("generating"), "{prompt}");
    }

    #[test]
    fn opening_the_generate_window_offers_the_levels_own_extent() {
        let mut e = a_working_editor();
        e.level_size = Some(Vec2::new(7000.0, 4000.0));
        e.last_save_hash = e.compute_entities_hash();
        e.request_action(PendingAction::GenerateLevel);
        assert_eq!(e.generate_dialog.extent, Vec2::new(7000.0, 4000.0));
    }

    // A17 — play mode starts at the level's own spawn when it has one.
    #[test]
    fn play_starts_at_the_levels_spawn_when_it_carries_one() {
        let mut e = an_editor_looking_at(
            egui::Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
            Vec2::new(800.0, 400.0),
        );
        e.level_spawn = Some(Pos2::new(1200.0, 700.0));

        e.start_play();

        assert_eq!(
            e.play_spawn,
            Some(Pos2::new(1200.0, 700.0)),
            "a generated level begins where it was proven to begin, not at the view centre"
        );
    }

    #[test]
    fn play_still_starts_at_the_view_centre_when_the_level_carries_no_spawn() {
        let canvas = egui::Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let mut e = an_editor_looking_at(canvas, Vec2::new(800.0, 400.0));
        assert_eq!(e.level_spawn, None, "a hand-built level has no spawn");

        e.start_play();

        let expected = canvas.center() + Vec2::new(800.0, 400.0);
        assert_eq!(e.play_spawn, Some(expected), "the existing behaviour is untouched");
    }

    #[test]
    fn a_click_during_play_still_moves_the_spawn_on_a_level_that_has_one() {
        let canvas = egui::Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let mut e = an_editor_looking_at(canvas, Vec2::ZERO);
        e.level_spawn = Some(Pos2::new(1200.0, 700.0));
        e.start_play();

        e.move_play_spawn(Pos2::new(300.0, 250.0), canvas);

        assert_eq!(
            e.play_spawn,
            Some(Pos2::new(300.0, 250.0)),
            "clicking during play moves the spawn as it always did"
        );
    }

    #[test]
    fn a_new_level_clears_the_spawn_and_exit_a_generated_level_brought() {
        let mut e = a_working_editor();
        e.level_spawn = Some(Pos2::new(1200.0, 700.0));
        e.level_exit = Some(Pos2::new(4200.0, 700.0));

        e.new_level();

        assert_eq!(e.level_spawn, None);
        assert_eq!(e.level_exit, None);
    }

    // ── Minimap mouse navigation ─────────────────────────────────────────────

    const CANVAS: egui::Rect = egui::Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(1280.0, 760.0));

    /// Run one headless egui frame with the given pointer events and hand the resulting
    /// pointer state to the minimap handler, exactly as `update` does.
    fn minimap_frame(e: &mut EditorState, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::PointerState {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0))),
            events,
            ..Default::default()
        };
        ctx.begin_frame(raw);
        let pointer = ctx.input(|i| i.pointer.clone());
        e.minimap_layout = minimap::layout(CANVAS, ctx.screen_rect(), e.resolved_level_size());
        e.handle_minimap_pointer(ctx, &pointer, CANVAS);
        let _ = ctx.end_frame();
        pointer
    }

    fn press(pos: Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]
    }

    fn release(pos: Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]
    }

    /// A wide level with an entity selected and its drag armed, as it is right after a
    /// Select-tool click.
    fn a_minimap_editor() -> EditorState {
        let mut e = EditorState::default();
        e.entities = vec![a_polygon(), a_polygon()];
        e.level_size = Some(Vec2::new(25600.0, 720.0));
        e.selected_entity = Some(1);
        e.dragging_entity = true;
        e.drag_start = Some(Pos2::new(300.0, 300.0));
        e.scroll = ScrollModel { offset: Vec2::ZERO, velocity: Vec2::new(900.0, 0.0) };
        e.scroll_offset = e.scroll.pixel_offset();
        e.last_save_hash = e.compute_entities_hash();
        e
    }

    #[test]
    fn a_press_on_the_minimap_jumps_the_view_and_kills_momentum() {
        let ctx = egui::Context::default();
        let mut e = a_minimap_editor();
        let layout = minimap::layout(CANVAS, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0)), e.resolved_level_size()).unwrap();
        let middle = layout.inner.center();

        minimap_frame(&mut e, &ctx, press(middle));
        assert!(e.minimap_drag, "the gesture is armed");
        assert!((e.scroll.offset.x - (12800.0 - CANVAS.width() / 2.0)).abs() < 2.0, "centred on the level middle, got {}", e.scroll.offset.x);
        assert_eq!(e.scroll.velocity, Vec2::ZERO, "arrow-key momentum discarded");
        assert_eq!(e.scroll_offset, e.scroll.pixel_offset(), "renderers see it this frame");
    }

    #[test]
    fn a_minimap_press_drops_a_pending_entity_drag_and_leaves_the_level_alone() {
        let ctx = egui::Context::default();
        let mut e = a_minimap_editor();
        let before = e.compute_entities_hash();
        let layout = minimap::layout(CANVAS, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0)), e.resolved_level_size()).unwrap();
        let start = layout.inner.center();

        minimap_frame(&mut e, &ctx, press(start));
        assert!(!e.dragging_entity && !e.dragging_polygon_point, "the armed drag is dropped at the press");
        assert_eq!(e.drag_start, None);
        minimap_frame(&mut e, &ctx, vec![egui::Event::PointerMoved(start + Vec2::new(200.0, 0.0))]);
        minimap_frame(&mut e, &ctx, release(start + Vec2::new(200.0, 0.0)));
        assert_eq!(e.compute_entities_hash(), before, "nothing moved");
        assert!(!e.has_unsaved_changes(), "and nothing counts as an edit");
    }

    #[test]
    fn the_gesture_is_consumed_through_the_release_frame_wherever_it_lands() {
        let ctx = egui::Context::default();
        let mut e = a_minimap_editor();
        let layout = minimap::layout(CANVAS, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0)), e.resolved_level_size()).unwrap();
        // Press on the frame, release 3 px above the box: egui still calls that a click.
        let start = Pos2::new(layout.inner.center().x, layout.map_rect.top() + 1.0);
        let off_box = start - Vec2::new(0.0, 4.0);
        assert!(!layout.contains(off_box));

        minimap_frame(&mut e, &ctx, press(start));
        let pointer = minimap_frame(&mut e, &ctx, release(off_box));
        assert!(pointer.primary_released());
        assert!(e.minimap_drag, "still the minimap's gesture on the release frame");
        assert!(e.pointer_over_ui(&ctx, off_box), "so the click is consumed, not delivered to a tool");

        // The frame after, the gesture is over and the same spot is canvas again.
        let pointer = minimap_frame(&mut e, &ctx, vec![egui::Event::PointerMoved(off_box)]);
        assert!(!pointer.primary_down() && !pointer.primary_released());
        assert!(!e.minimap_drag);
        assert!(!e.pointer_over_ui(&ctx, off_box));
    }

    #[test]
    fn a_drag_that_started_on_the_canvas_does_not_move_the_view_over_the_minimap() {
        let ctx = egui::Context::default();
        let mut e = a_minimap_editor();
        let layout = minimap::layout(CANVAS, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0)), e.resolved_level_size()).unwrap();
        let canvas_spot = Pos2::new(200.0, 200.0);
        assert!(!layout.contains(canvas_spot));

        minimap_frame(&mut e, &ctx, press(canvas_spot));
        assert!(!e.minimap_drag);
        assert!(e.dragging_entity, "a canvas press leaves the entity drag alone");
        minimap_frame(&mut e, &ctx, vec![egui::Event::PointerMoved(layout.inner.center())]);
        assert!(!e.minimap_drag);
        assert_eq!(e.scroll.offset, Vec2::ZERO, "the view stayed put");
    }

    #[test]
    fn the_minimap_ignores_the_mouse_while_playing_but_still_owns_the_spot() {
        let ctx = egui::Context::default();
        let mut e = a_minimap_editor();
        e.play_spawn = Some(Pos2::new(100.0, 100.0));
        e.toggle_play();
        assert!(e.play.is_some());
        let offset_before = e.scroll.offset;
        let layout = minimap::layout(CANVAS, egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 760.0)), e.resolved_level_size()).unwrap();
        let middle = layout.inner.center();

        minimap_frame(&mut e, &ctx, press(middle));
        assert_eq!(e.scroll.offset, offset_before, "the camera keeps the view");
        assert!(e.pointer_over_ui(&ctx, middle), "but the press is still consumed, so it cannot move the spawn");

        // Even when the button comes up a few pixels off the box, where egui still
        // reports a click: the release frame is still the minimap's.
        let off_box = Pos2::new(middle.x, layout.map_rect.top() - 4.0);
        assert!(!layout.contains(off_box));
        minimap_frame(&mut e, &ctx, vec![egui::Event::PointerMoved(off_box)]);
        let pointer = minimap_frame(&mut e, &ctx, release(off_box));
        assert!(pointer.primary_released());
        assert_eq!(e.scroll.offset, offset_before, "still no view change");
        assert!(e.pointer_over_ui(&ctx, off_box), "the click at the release spot is consumed, not a spawn move");
        minimap_frame(&mut e, &ctx, vec![egui::Event::PointerMoved(off_box)]);
        assert!(!e.pointer_over_ui(&ctx, off_box), "and the frame after, the canvas is itself again");
    }

    #[test]
    fn a_new_level_clears_the_level_that_was_open() {
        let mut e = a_working_editor();
        e.new_level();

        assert!(e.entities.is_empty(), "no entities");
        assert_eq!(e.level_size, None, "no explicit size, so the extent resolves again");
        assert_eq!(e.background_size, Vec2::ZERO, "no background size");
        assert!(!e.background_controller.has_image(), "and no background image");
        assert_eq!(e.play_spawn, None, "no spawn point");
        assert_eq!(e.last_click_pos, None, "and no click for play to spawn from");
        assert_eq!(e.selected_entity, None, "nothing selected");
        assert_eq!(e.editing_polygon_entity, None);
        assert_eq!(e.editing_polygon_point, None, "no polygon being edited");
        assert!(!e.dragging_entity && !e.dragging_polygon_point, "no drag in progress");
        assert!(e.drawing_polygon.is_none(), "no polygon being drawn");
        assert!(e.undo_stack.is_empty() && e.redo_stack.is_empty(), "no history");
        assert!(e.size_dialog.is_none(), "the Level Size dialog is closed");
    }

    #[test]
    fn a_new_level_puts_the_view_back_at_the_origin() {
        let mut e = a_working_editor();
        e.new_level();
        assert_eq!(e.scroll.offset, Vec2::ZERO, "view at the origin");
        assert_eq!(e.scroll.velocity, Vec2::ZERO, "and stationary, not still gliding");
        assert_eq!(e.scroll_offset, Vec2::ZERO, "including the offset renderers read");
    }

    #[test]
    fn a_new_level_reads_as_clean_so_quitting_does_not_prompt() {
        let mut e = a_working_editor();
        e.entities.push(a_polygon());
        assert!(e.has_unsaved_changes(), "the working editor is dirty to begin with");
        e.new_level();
        assert!(!e.has_unsaved_changes(), "a fresh level is not unsaved work");
    }

    #[test]
    fn a_new_level_forgets_the_file_that_was_open() {
        let mut e = a_working_editor();
        e.new_level();
        assert_eq!(e.last_level_path, None, "so Save Level asks for a destination");
        assert_eq!(e.last_background_path, None);
    }

    #[test]
    fn dirtying_a_new_level_is_noticed() {
        // The clean marker is not a blanket "always clean": the first edit after the
        // reset registers.
        let mut e = a_working_editor();
        e.new_level();
        e.entities.push(a_polygon());
        assert!(e.has_unsaved_changes(), "an entity added after the reset is unsaved work");

        e.entities.clear();
        e.level_size = Some(Vec2::new(2000.0, 1500.0));
        assert!(e.has_unsaved_changes(), "and so is setting the level size");
    }

    // ── Play spawns at the centre of the visible view ────────────────────────

    /// An editor whose canvas has been laid out and whose view has been scrolled, as it
    /// is on any frame after the first. `canvas` is the rectangle the canvas occupies on
    /// screen, `offset` the scroll offset, so the visible world region is the canvas
    /// translated by the offset.
    fn an_editor_looking_at(canvas: egui::Rect, offset: Vec2) -> EditorState {
        let mut e = EditorState::default();
        e.entities = vec![a_polygon()];
        e.level_size = Some(Vec2::new(25600.0, 4000.0));
        e.canvas_rect = Some(canvas);
        e.scroll = ScrollModel { offset, velocity: Vec2::ZERO };
        e.scroll_offset = e.scroll.pixel_offset();
        e
    }

    /// A canvas 800x600 sitting under the menu bar, scrolled so it shows world
    /// x 800..1600 and y 400..1000.
    fn a_view_of_the_middle_of_the_level() -> EditorState {
        an_editor_looking_at(
            egui::Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(800.0, 640.0)),
            Vec2::new(800.0, 360.0),
        )
    }

    #[test]
    fn play_starts_the_character_at_the_centre_of_the_visible_canvas() {
        let mut e = a_view_of_the_middle_of_the_level();
        e.start_play();

        let play = e.play.as_ref().expect("play mode started");
        assert_eq!(
            play.player.pos,
            Pos2::new(1200.0, 700.0),
            "the character starts at the centre of the world the canvas is showing"
        );
        assert_eq!(e.play_spawn, Some(Pos2::new(1200.0, 700.0)), "and that is the spawn point");
    }

    #[test]
    fn play_starts_at_the_centre_with_the_view_at_the_level_origin() {
        let mut e = an_editor_looking_at(
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0)),
            Vec2::ZERO,
        );
        e.start_play();

        assert_eq!(
            e.play.as_ref().expect("play mode started").player.pos,
            Pos2::new(400.0, 300.0),
            "an unscrolled view of world 0..800 by 0..600 starts the character in its middle"
        );
    }

    #[test]
    fn the_centre_is_the_canvas_centre_not_the_window_centre() {
        // The canvas sits below the menu bar. Measuring the whole window instead would
        // put the spawn 20px high, which is exactly the bug this guards.
        let mut e = an_editor_looking_at(CANVAS, Vec2::ZERO);
        e.start_play();

        assert_eq!(
            e.play.as_ref().expect("play mode started").player.pos,
            Pos2::new(640.0, 400.0),
            "the menu bar is not part of the canvas, so it does not pull the centre up"
        );
    }

    #[test]
    fn a_click_made_before_play_no_longer_decides_where_the_run_begins() {
        let mut e = a_view_of_the_middle_of_the_level();
        // Clicked at the origin, then scrolled far away so that point is off screen.
        e.last_click_pos = Some(Pos2::new(50.0, 50.0));
        e.start_play();

        let pos = e.play.as_ref().expect("play mode started").player.pos;
        assert_eq!(pos, Pos2::new(1200.0, 700.0), "the view decides, not the stale click");
        assert_ne!(pos, Pos2::new(50.0, 50.0), "and certainly not a point that is off screen");
    }

    #[test]
    fn restarting_the_run_returns_the_character_to_the_view_centre() {
        let mut e = a_view_of_the_middle_of_the_level();
        e.start_play();
        let centre = Pos2::new(1200.0, 700.0);

        let play = e.play.as_mut().expect("play mode started");
        play.player.pos = Pos2::new(4000.0, 2000.0);
        play.restart();

        assert_eq!(
            e.play.as_ref().unwrap().player.pos,
            centre,
            "R puts the character back where the run began"
        );
    }

    #[test]
    fn a_click_while_playing_still_moves_the_spawn_and_respawns_there() {
        let mut e = a_view_of_the_middle_of_the_level();
        e.start_play();

        let clicked = Pos2::new(900.0, 500.0);
        e.move_play_spawn(clicked, CANVAS);

        assert_eq!(e.play_spawn, Some(clicked), "the click moved the spawn point");
        assert_eq!(
            e.play.as_ref().expect("still playing").player.pos,
            clicked,
            "and the character restarted there"
        );
    }

    #[test]
    fn the_play_menu_and_f5_start_in_the_same_place() {
        // Both go through `toggle_play`, so one run of it stands for both.
        let mut e = a_view_of_the_middle_of_the_level();
        e.toggle_play();
        let first = e.play.as_ref().expect("play mode started").player.pos;

        e.toggle_play();
        assert!(e.play.is_none(), "the second toggle stops play");
        e.toggle_play();
        let second = e.play.as_ref().expect("play mode started again").player.pos;

        assert_eq!(first, Pos2::new(1200.0, 700.0));
        assert_eq!(second, first, "the same view starts the character in the same place");
    }

    #[test]
    fn stopping_play_restores_the_view_it_began_with() {
        let mut e = a_view_of_the_middle_of_the_level();
        let before = e.scroll.offset;
        e.start_play();

        // The camera wanders while the character runs.
        e.scroll.offset = Vec2::new(9000.0, 1200.0);
        e.stop_play();

        assert!(e.play.is_none(), "play stopped");
        assert_eq!(e.scroll.offset, before, "and the editor is looking where it was");
    }

    #[test]
    fn without_a_laid_out_canvas_play_falls_back_to_the_last_spawn() {
        // Before the first frame there is no canvas to take a centre from.
        let mut e = EditorState::default();
        e.play_spawn = Some(Pos2::new(120.0, 340.0));
        e.start_play();
        assert_eq!(
            e.play.as_ref().expect("play mode started").player.pos,
            Pos2::new(120.0, 340.0),
            "the remembered spawn stands in"
        );

        let mut blank = EditorState::default();
        blank.start_play();
        assert_eq!(
            blank.play.as_ref().expect("play mode started").player.pos,
            Pos2::new(100.0, 100.0),
            "and with nothing remembered the spawn is still defined"
        );
    }
}
