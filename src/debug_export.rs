//! Optional JSON export for MCP-assisted debugging (see `mcp_rust_game_editor/README.md`).
use serde::Serialize;

/// Snapshot of minimap layout state, written to the cache dir when debug export is enabled.
#[derive(Debug, Clone, Serialize)]
pub struct MinimapDebugSnapshot {
    pub timestamp_secs: f64,
    pub early_exit: bool,
    pub early_exit_reason: Option<String>,
    pub panel_clip_min: [f32; 2],
    pub panel_clip_max: [f32; 2],
    pub panel_clip_size: [f32; 2],
    pub level: [f32; 2],
    pub map_w: f32,
    pub map_h: f32,
    pub map_rect_min: [f32; 2],
    pub map_rect_max: [f32; 2],
    pub inner_rect_min: [f32; 2],
    pub inner_rect_max: [f32; 2],
    pub has_minimap_texture: bool,
    pub scroll_offset: [f32; 2],
    pub screen_rect_min: [f32; 2],
    pub screen_rect_max: [f32; 2],
    pub pixels_per_point: f32,
}

impl MinimapDebugSnapshot {
    pub fn early_only(
        timestamp_secs: f64,
        panel: egui::Rect,
        reason: &str,
        screen: egui::Rect,
        pixels_per_point: f32,
    ) -> Self {
        Self {
            timestamp_secs,
            early_exit: true,
            early_exit_reason: Some(reason.to_string()),
            panel_clip_min: [panel.min.x, panel.min.y],
            panel_clip_max: [panel.max.x, panel.max.y],
            panel_clip_size: [panel.width(), panel.height()],
            level: [0.0, 0.0],
            map_w: 0.0,
            map_h: 0.0,
            map_rect_min: [0.0, 0.0],
            map_rect_max: [0.0, 0.0],
            inner_rect_min: [0.0, 0.0],
            inner_rect_max: [0.0, 0.0],
            has_minimap_texture: false,
            scroll_offset: [0.0, 0.0],
            screen_rect_min: [screen.min.x, screen.min.y],
            screen_rect_max: [screen.max.x, screen.max.y],
            pixels_per_point,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn full(
        timestamp_secs: f64,
        panel: egui::Rect,
        level: egui::Vec2,
        map_w: f32,
        map_h: f32,
        map_rect: egui::Rect,
        inner: egui::Rect,
        has_minimap_texture: bool,
        scroll_offset: egui::Vec2,
        screen: egui::Rect,
        pixels_per_point: f32,
    ) -> Self {
        Self {
            timestamp_secs,
            early_exit: false,
            early_exit_reason: None,
            panel_clip_min: [panel.min.x, panel.min.y],
            panel_clip_max: [panel.max.x, panel.max.y],
            panel_clip_size: [panel.width(), panel.height()],
            level: [level.x, level.y],
            map_w,
            map_h,
            map_rect_min: [map_rect.min.x, map_rect.min.y],
            map_rect_max: [map_rect.max.x, map_rect.max.y],
            inner_rect_min: [inner.min.x, inner.min.y],
            inner_rect_max: [inner.max.x, inner.max.y],
            has_minimap_texture,
            scroll_offset: [scroll_offset.x, scroll_offset.y],
            screen_rect_min: [screen.min.x, screen.min.y],
            screen_rect_max: [screen.max.x, screen.max.y],
            pixels_per_point,
        }
    }
}

/// Enabled when `RUST_GAME_EDITOR_DEBUG=1`, or in debug builds (`cfg!(debug_assertions)`).
pub fn minimap_debug_export_enabled() -> bool {
    if std::env::var("RUST_GAME_EDITOR_DEBUG")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return true;
    }
    cfg!(debug_assertions)
}

/// Writes `editor_debug.json` under the OS cache directory (`dirs::cache_dir()/rust_game_editor/`).
pub fn write_minimap_debug_json(snapshot: &MinimapDebugSnapshot) {
    if !minimap_debug_export_enabled() {
        return;
    }
    let Some(cache) = dirs::cache_dir() else {
        return;
    };
    let dir = cache.join("rust_game_editor");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("editor_debug.json");
    if let Ok(json) = serde_json::to_string_pretty(snapshot) {
        let _ = std::fs::write(path, json);
    }
}
