use egui::{Color32, Pos2, Vec2};
use crate::toolbox::ToolboxLayout;
use crate::Asset;
use crate::level_data::LevelEntity;

/// Parse a hex color string (e.g., "#FF0000") to egui Color32
fn parse_hex_color(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    
    Some(Color32::from_rgb(r, g, b))
}

pub trait DrawableEntity {
    fn draw(&self, painter: &egui::Painter, scroll_offset: Vec2, is_selected: bool);
    fn contains_point(&self, point: Pos2) -> bool;
    fn translate(&mut self, delta: Vec2);
    fn entity_type(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct BitmapEntity {
    pub pos: Pos2,
    pub bitmap_name: String,
    bitmap_size: Vec2,
}

impl BitmapEntity {
    pub fn new(pos: Pos2, bitmap_name: String, toolbox_layout: Option<&ToolboxLayout>) -> Self {
        let bitmap_size = Self::get_bitmap_size(&bitmap_name, toolbox_layout);
        Self {
            pos,
            bitmap_name,
            bitmap_size,
        }
    }

    fn get_bitmap_size(bitmap_name: &str, toolbox_layout: Option<&ToolboxLayout>) -> Vec2 {
        if let Some(layout) = toolbox_layout {
            if let Some(tool_def) = layout.tools.iter().find(|t| &t.name == bitmap_name) {
                let asset_path = tool_def.icon.replace("assets/", "");
                if let Some(bytes) = Asset::get(&asset_path) {
                    if let Ok(img) = image::load_from_memory(bytes.data.as_ref()) {
                        return Vec2::new(img.width() as f32, img.height() as f32);
                    }
                }
            }
        }
        Vec2::new(32.0, 32.0) // fallback
    }

    pub fn get_size(&self) -> Vec2 {
        self.bitmap_size
    }
}

impl DrawableEntity for BitmapEntity {
    fn draw(&self, painter: &egui::Painter, scroll_offset: Vec2, is_selected: bool) {
        // Drawing is handled externally since we need texture handles
        // This is a placeholder for the interface
        let rect = egui::Rect::from_min_size(self.pos - scroll_offset, self.bitmap_size);
        if is_selected {
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, Color32::GREEN));
        }
    }

    fn contains_point(&self, point: Pos2) -> bool {
        let rect = egui::Rect::from_min_size(self.pos, self.bitmap_size);
        rect.contains(point)
    }

    fn translate(&mut self, delta: Vec2) {
        self.pos += delta;
    }

    fn entity_type(&self) -> &str {
        "bitmap"
    }
}

#[derive(Clone, Debug)]
pub struct PolygonEntity {
    pub points: Vec<Pos2>,
    pub polygon_type: String, // e.g., "wall", "blocker", "polygon"
    pub color: Option<String>, // Optional hex color like "#FF0000"
}

impl PolygonEntity {
    pub fn new(points: Vec<Pos2>) -> Self {
        Self { points, polygon_type: "polygon".to_string(), color: None }
    }
    
    pub fn with_type(points: Vec<Pos2>, polygon_type: String) -> Self {
        Self { points, polygon_type, color: None }
    }
    
    pub fn with_type_and_color(points: Vec<Pos2>, polygon_type: String, color: Option<String>) -> Self {
        Self { points, polygon_type, color }
    }
}

impl DrawableEntity for PolygonEntity {
    fn draw(&self, painter: &egui::Painter, scroll_offset: Vec2, is_selected: bool) {
        let line_color = if is_selected {
            Color32::GREEN
        } else if let Some(color_hex) = &self.color {
            parse_hex_color(color_hex).unwrap_or(Color32::BLUE)
        } else if self.polygon_type == "wall" {
            Color32::RED
        } else {
            Color32::BLUE
        };
        let point_color = if is_selected { Color32::YELLOW } else { Color32::RED };
        
        for i in 0..self.points.len() {
            let a = self.points[i] - scroll_offset;
            let b = self.points[(i + 1) % self.points.len()] - scroll_offset;
            painter.line_segment([a, b], (2.0, line_color));
            painter.circle_filled(a, 4.0, point_color);
        }
    }

    fn contains_point(&self, point: Pos2) -> bool {
        if self.points.len() < 2 {
            return false;
        }
        
        let hit_distance = 8.0; // Distance threshold for edge/vertex selection
        
        // Check if near any vertex
        for p in &self.points {
            if (point - *p).length() < hit_distance {
                return true;
            }
        }
        
        // Check if near any edge
        let n = self.points.len();
        for i in 0..n {
            let p1 = self.points[i];
            let p2 = self.points[(i + 1) % n];
            
            // Calculate distance from point to line segment
            let line = p2 - p1;
            let line_len_sq = line.length_sq();
            if line_len_sq < 0.001 {
                continue; // Skip degenerate edges
            }
            
            let t = ((point - p1).dot(line) / line_len_sq).clamp(0.0, 1.0);
            let projection = p1 + t * line;
            let distance = (point - projection).length();
            
            if distance < hit_distance {
                return true;
            }
        }
        
        // For filled polygons with 3+ points, also check if inside using ray casting
        if self.points.len() >= 3 {
            let mut inside = false;
            for i in 0..n {
                let p1 = self.points[i];
                let p2 = self.points[(i + 1) % n];
                
                if (p1.y > point.y) != (p2.y > point.y) {
                    let x_intersect = (p2.x - p1.x) * (point.y - p1.y) / (p2.y - p1.y) + p1.x;
                    if point.x < x_intersect {
                        inside = !inside;
                    }
                }
            }
            if inside {
                return true;
            }
        }
        
        false
    }

    fn translate(&mut self, delta: Vec2) {
        for point in &mut self.points {
            *point += delta;
        }
    }

    fn entity_type(&self) -> &str {
        "polygon"
    }
}

#[derive(Clone, Debug)]
pub enum Entity {
    Bitmap(BitmapEntity),
    Polygon(PolygonEntity),
}

impl Entity {
    pub fn new_bitmap(pos: Pos2, bitmap_name: String, toolbox_layout: Option<&ToolboxLayout>) -> Self {
        Entity::Bitmap(BitmapEntity::new(pos, bitmap_name, toolbox_layout))
    }

    pub fn new_polygon(points: Vec<Pos2>) -> Self {
        Entity::Polygon(PolygonEntity::new(points))
    }
    
    pub fn new_polygon_with_type(points: Vec<Pos2>, polygon_type: String) -> Self {
        Entity::Polygon(PolygonEntity::with_type(points, polygon_type))
    }
    
    pub fn new_polygon_with_type_and_color(points: Vec<Pos2>, polygon_type: String, color: Option<String>) -> Self {
        Entity::Polygon(PolygonEntity::with_type_and_color(points, polygon_type, color))
    }

    pub fn as_bitmap(&self) -> Option<&BitmapEntity> {
        match self {
            Entity::Bitmap(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_polygon(&self) -> Option<&PolygonEntity> {
        match self {
            Entity::Polygon(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_bitmap_mut(&mut self) -> Option<&mut BitmapEntity> {
        match self {
            Entity::Bitmap(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_polygon_mut(&mut self) -> Option<&mut PolygonEntity> {
        match self {
            Entity::Polygon(p) => Some(p),
            _ => None,
        }
    }

    /// Convert this entity to a LevelEntity for serialization
    pub fn to_level_entity(&self) -> LevelEntity {
        match self {
            Entity::Bitmap(bitmap) => {
                LevelEntity::Bitmap {
                    position: [bitmap.pos.x, bitmap.pos.y],
                    bitmap_name: bitmap.bitmap_name.clone(),
                    size: [bitmap.bitmap_size.x, bitmap.bitmap_size.y],
                }
            }
            Entity::Polygon(polygon) => {
                let vertices: Vec<[f32; 2]> = polygon.points
                    .iter()
                    .map(|p| [p.x, p.y])
                    .collect();
                
                LevelEntity::Polygon {
                    vertices,
                    polygon_type: Some(polygon.polygon_type.clone()),
                    color: polygon.color.clone(),
                }
            }
        }
    }

    /// Create an entity from a LevelEntity
    pub fn from_level_entity(level_entity: &LevelEntity, toolbox_layout: Option<&ToolboxLayout>) -> Self {
        match level_entity {
            LevelEntity::Bitmap { position, bitmap_name, .. } => {
                Entity::new_bitmap(
                    Pos2::new(position[0], position[1]),
                    bitmap_name.clone(),
                    toolbox_layout,
                )
            }
            LevelEntity::Polygon { vertices, polygon_type, color } => {
                let points: Vec<Pos2> = vertices
                    .iter()
                    .map(|v| Pos2::new(v[0], v[1]))
                    .collect();
                let poly_type = polygon_type.as_deref().unwrap_or("polygon").to_string();
                Entity::new_polygon_with_type_and_color(points, poly_type, color.clone())
            }
        }
    }
}

impl DrawableEntity for Entity {
    fn draw(&self, painter: &egui::Painter, scroll_offset: Vec2, is_selected: bool) {
        match self {
            Entity::Bitmap(b) => b.draw(painter, scroll_offset, is_selected),
            Entity::Polygon(p) => p.draw(painter, scroll_offset, is_selected),
        }
    }

    fn contains_point(&self, point: Pos2) -> bool {
        match self {
            Entity::Bitmap(b) => b.contains_point(point),
            Entity::Polygon(p) => p.contains_point(point),
        }
    }

    fn translate(&mut self, delta: Vec2) {
        match self {
            Entity::Bitmap(b) => b.translate(delta),
            Entity::Polygon(p) => p.translate(delta),
        }
    }

    fn entity_type(&self) -> &str {
        match self {
            Entity::Bitmap(b) => b.entity_type(),
            Entity::Polygon(p) => p.entity_type(),
        }
    }
}
