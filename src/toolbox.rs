use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use egui::{Color32, Pos2};
use tracing::{error, trace, warn};

#[derive(Deserialize, Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub tool_type: String,
    pub icon: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ToolboxLayout {
    pub tools: Vec<ToolDef>,
    pub main_toolbox: MainToolbox,
}

#[derive(Deserialize, Debug)]
pub struct MainToolbox {
    #[allow(dead_code)]
    pub name: String,
    pub tools: Vec<Vec<String>>,
}

pub fn load_toolbox_layout() -> Option<ToolboxLayout> {
    match fs::read_to_string("src/toolboxes.json") {
        Ok(data) => {
            trace!("toolbox_layout_file_read bytes={}", data.len());
            match serde_json::from_str::<ToolboxLayout>(&data) {
                Ok(layout) => {
                    trace!("toolbox_layout_parsed tool_count={}", layout.tools.len());
                    Some(layout)
                }
                Err(e) => {
                    error!("toolbox_layout_parse_error error={}", e);
                    None
                }
            }
        }
        Err(e) => {
            error!("toolbox_layout_read_error error={}", e);
            None
        }
    }
}

pub fn render_toolbox(
    ctx: &egui::Context,
    layout: &ToolboxLayout,
    toolbox_pos: Pos2,
    loaded_textures: &mut HashMap<String, egui::TextureHandle>,
    current_tool: &Option<String>,
) -> Option<String> {
    trace!("render_toolbox_start pos={:?} textures_loaded={} rows={}", toolbox_pos, loaded_textures.len(), layout.main_toolbox.tools.len());
    let rows = &layout.main_toolbox.tools;
    let thumb_size = egui::vec2(40.0, 40.0);
    let toolbox_height = rows.len() as f32 * thumb_size.y;
    let toolbox_width = thumb_size.x * rows.iter().map(|r| r.len()).max().unwrap_or(1) as f32;
    let toolbox_rect = egui::Rect::from_min_size(toolbox_pos, egui::vec2(toolbox_width, toolbox_height));
    
    let mut tool_selected = None;
    
    egui::Area::new("toolbox".into())
        .movable(false)
        .fixed_pos(toolbox_pos)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_gray(180))
                .inner_margin(egui::Margin::same(4.0))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        for row in rows {
                            ui.horizontal(|ui| {
                                for tool_name in row {
                                    if let Some(tool_def) = layout.tools.iter().find(|t| &t.name == tool_name) {
                                        let texture_handle = if let Some(tex) = loaded_textures.get(&tool_def.icon) {
                                            tex.clone()
                                        } else {
                                            load_tool_texture(ctx, tool_def, loaded_textures)
                                        };
                                        
                                        // Check if this tool is currently selected
                                        let is_selected = current_tool.as_ref() == Some(&tool_def.name);
                                        
                                        let resp = ui.add(
                                            egui::Image::new(&texture_handle)
                                                .fit_to_exact_size(thumb_size)
                                                .sense(egui::Sense::click())
                                        );

                                        // Draw frame for selected tool, and additional feedback for hover/press
                                        let frame_stroke = if is_selected {
                                            egui::Stroke::new(3.0, Color32::YELLOW)
                                        } else if resp.is_pointer_button_down_on() {
                                            egui::Stroke::new(2.0, Color32::WHITE)
                                        } else if resp.hovered() {
                                            egui::Stroke::new(1.0, Color32::LIGHT_GRAY)
                                        } else {
                                            egui::Stroke::NONE
                                        };
                                        
                                        // Only fill on hover/press, not when selected (so thumbnail shows clearly)
                                        let fill = if resp.is_pointer_button_down_on() {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 40)
                                        } else if resp.hovered() {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 20)
                                        } else {
                                            Color32::TRANSPARENT
                                        };
                                        
                                        ui.painter().rect_filled(resp.rect.shrink(1.0), 4.0, fill);
                                        ui.painter().rect_stroke(resp.rect.shrink(1.0), 4.0, frame_stroke);

                                        if resp.clicked() {
                                            tool_selected = Some(tool_def.name.clone());
                                            trace!("toolbox_tool_clicked name={}", tool_def.name);
                                        }
                                        if resp.hovered() {
                                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                            egui::show_tooltip_at_pointer(ctx, egui::Id::new(&tool_def.name), |ui| {
                                                ui.label(&tool_def.name);
                                            });
                                        }
                                    } else {
                                        ui.label(tool_name);
                                    }
                                }
                            });
                        }
                    });
                });
            let _response = ui.interact(toolbox_rect, ui.id(), egui::Sense::click_and_drag());
        });
    
    trace!("render_toolbox_end selection={:?}", tool_selected);
    tool_selected
}

fn load_tool_texture(
    ctx: &egui::Context,
    tool_def: &ToolDef,
    loaded_textures: &mut HashMap<String, egui::TextureHandle>,
) -> egui::TextureHandle {
    use rust_embed::RustEmbed;
    
    #[derive(RustEmbed)]
    #[folder = "src/assets"]
    struct Asset;
    
    let asset_path = tool_def.icon.replace("assets/", "");
    trace!("load_tool_texture_start icon={} asset_path={}", tool_def.icon, asset_path);
    
    if let Some(bytes) = Asset::get(&asset_path) {
        trace!("load_tool_texture_bytes icon={} bytes={} name={}", tool_def.icon, bytes.data.len(), tool_def.name);
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
            let tex = ctx.load_texture(&tool_def.icon, image_data, egui::TextureOptions::default());
            loaded_textures.insert(tool_def.icon.clone(), tex.clone());
            trace!("load_tool_texture_success icon={} size={:?}", tool_def.icon, size);
            return tex;
        } else {
            warn!("load_tool_texture_decode_failed icon={} asset_path={}", tool_def.icon, asset_path);
        }
    } else {
        warn!("load_tool_texture_asset_missing asset_path={}", asset_path);
    }
    
    // Fallback gray texture
    let image_data = egui::ImageData::Color(egui::ColorImage {
        size: [32, 32],
        pixels: vec![Color32::GRAY; 32 * 32],
    }.into());
    let tex = ctx.load_texture(&tool_def.icon, image_data, egui::TextureOptions::default());
    loaded_textures.insert(tool_def.icon.clone(), tex.clone());
    trace!("load_tool_texture_fallback icon={} size=32x32", tool_def.icon);
    tex
}

pub fn get_toolbox_rect(layout: &ToolboxLayout, pos: Pos2) -> egui::Rect {
    let rows = &layout.main_toolbox.tools;
    let toolbox_height = rows.len() as f32 * 40.0;
    let toolbox_width = 40.0 * rows.iter().map(|r| r.len()).max().unwrap_or(1) as f32;
    egui::Rect::from_min_size(pos, egui::vec2(toolbox_width, toolbox_height))
}
