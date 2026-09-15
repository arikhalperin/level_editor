use egui::{Color32, Pos2, Vec2};
use crate::toolbox::ToolboxLayout;
use crate::Asset;
use crate::level_data::LevelEntity;
use crate::game_config as cfg;

/// Distance from `p` to the segment `a`–`b`.
pub fn distance_to_segment(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    let t = if len_sq <= f32::EPSILON { 0.0 } else { ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

/// How close to a rope's line a click must be to hit it, px.
pub const ROPE_HIT_DISTANCE: f32 = 8.0;
/// Rope colour on the canvas.
pub const ROPE_COLOR: Color32 = Color32::from_rgb(139, 90, 43);

/// A rope hung from an anchor, vertical at rest. Saved as a `rope_tool` bitmap entry.
#[derive(Clone, Debug, PartialEq)]
pub struct RopeEntity {
    /// Top anchor in world space.
    pub anchor: Pos2,
    /// Length in world px, at least `ROPE_MIN_LENGTH`.
    pub length: f32,
}

impl RopeEntity {
    pub fn new(anchor: Pos2, length: f32) -> Self {
        Self { anchor, length: length.max(cfg::ROPE_MIN_LENGTH) }
    }

    /// The bottom end at rest.
    pub fn end(&self) -> Pos2 {
        self.anchor + Vec2::new(0.0, self.length)
    }

    /// Draw a rope from `anchor` along `dir` (unit) for `length`, in screen space.
    pub fn paint(painter: &egui::Painter, anchor: Pos2, dir: Vec2, length: f32, is_selected: bool) {
        let color = if is_selected { Color32::GREEN } else { ROPE_COLOR };
        let end = anchor + dir * length;
        painter.line_segment([anchor, end], (cfg::ROPE_THICKNESS, color));
        painter.circle_filled(anchor, cfg::ROPE_THICKNESS, color);
        painter.circle_stroke(anchor, cfg::ROPE_THICKNESS, (1.0, Color32::BLACK));
    }
}

/// What a Rope-tool click does, given the anchor already set (if any).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RopePlacement {
    /// First click: remember this anchor.
    SetAnchor(Pos2),
    /// Second click low enough: create the rope.
    Finish { anchor: Pos2, length: f32 },
    /// Second click above the anchor or too close: nothing happens.
    Ignore,
}

impl RopeEntity {
    /// The two-click placement rule: the first click is the anchor, the second sets the
    /// length from its height alone and must be at least `ROPE_MIN_LENGTH` lower.
    pub fn place(anchor: Option<Pos2>, click: Pos2) -> RopePlacement {
        match anchor {
            None => RopePlacement::SetAnchor(click),
            Some(a) => {
                let length = click.y - a.y;
                if length >= cfg::ROPE_MIN_LENGTH {
                    RopePlacement::Finish { anchor: a, length }
                } else {
                    RopePlacement::Ignore
                }
            }
        }
    }
}

impl RopeEntity {
    /// Draw a rope along `points` (screen space): the chain while playing.
    pub fn paint_polyline(painter: &egui::Painter, points: &[Pos2], is_selected: bool) {
        let color = if is_selected { Color32::GREEN } else { ROPE_COLOR };
        for w in points.windows(2) {
            painter.line_segment([w[0], w[1]], (cfg::ROPE_THICKNESS, color));
        }
        if let Some(anchor) = points.first() {
            painter.circle_filled(*anchor, cfg::ROPE_THICKNESS, color);
            painter.circle_stroke(*anchor, cfg::ROPE_THICKNESS, (1.0, Color32::BLACK));
        }
    }
}

impl DrawableEntity for RopeEntity {
    fn draw(&self, painter: &egui::Painter, scroll_offset: Vec2, is_selected: bool) {
        Self::paint(painter, self.anchor - scroll_offset, Vec2::new(0.0, 1.0), self.length, is_selected);
    }

    fn contains_point(&self, point: Pos2) -> bool {
        distance_to_segment(self.anchor, self.end(), point) <= ROPE_HIT_DISTANCE
    }

    fn translate(&mut self, delta: Vec2) {
        self.anchor += delta;
    }

    fn entity_type(&self) -> &str {
        "rope"
    }
}

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
    Rope(RopeEntity),
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

    pub fn new_rope(anchor: Pos2, length: f32) -> Self {
        Entity::Rope(RopeEntity::new(anchor, length))
    }

    pub fn as_rope(&self) -> Option<&RopeEntity> {
        match self {
            Entity::Rope(r) => Some(r),
            _ => None,
        }
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
            // A rope is a bitmap entry to every other consumer: anchor + [thickness, length].
            Entity::Rope(rope) => LevelEntity::Bitmap {
                position: [rope.anchor.x, rope.anchor.y],
                bitmap_name: cfg::ROPE_BITMAP.to_string(),
                size: [cfg::ROPE_THICKNESS, rope.length],
            },
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
            LevelEntity::Bitmap { position, bitmap_name, size } if bitmap_name == cfg::ROPE_BITMAP => {
                Entity::new_rope(Pos2::new(position[0], position[1]), size[1])
            }
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
            Entity::Rope(r) => r.draw(painter, scroll_offset, is_selected),
        }
    }

    fn contains_point(&self, point: Pos2) -> bool {
        match self {
            Entity::Bitmap(b) => b.contains_point(point),
            Entity::Polygon(p) => p.contains_point(point),
            Entity::Rope(r) => r.contains_point(point),
        }
    }

    fn translate(&mut self, delta: Vec2) {
        match self {
            Entity::Bitmap(b) => b.translate(delta),
            Entity::Polygon(p) => p.translate(delta),
            Entity::Rope(r) => r.translate(delta),
        }
    }

    fn entity_type(&self) -> &str {
        match self {
            Entity::Bitmap(b) => b.entity_type(),
            Entity::Polygon(p) => p.entity_type(),
            Entity::Rope(r) => r.entity_type(),
        }
    }
}

#[cfg(test)]
mod rope_tests {
    use super::*;

    #[test]
    fn a_rope_saves_as_a_rope_tool_bitmap_entry_and_loads_back() {
        let rope = Entity::new_rope(Pos2::new(400.0, 100.0), 300.0);
        let level = rope.to_level_entity();
        match &level {
            LevelEntity::Bitmap { position, bitmap_name, size } => {
                assert_eq!(*position, [400.0, 100.0]);
                assert_eq!(bitmap_name, "rope_tool");
                assert_eq!(*size, [cfg::ROPE_THICKNESS, 300.0]);
            }
            other => panic!("expected a bitmap entry, got {other:?}"),
        }
        let json = serde_json::to_string(&level).unwrap();
        assert!(json.contains("\"type\":\"bitmap\""), "plain bitmap on the wire: {json}");
        let back = Entity::from_level_entity(&serde_json::from_str(&json).unwrap(), None);
        let r = back.as_rope().expect("loads back as a rope");
        assert_eq!(r.anchor, Pos2::new(400.0, 100.0));
        assert_eq!(r.length, 300.0);
    }

    #[test]
    fn a_too_short_rope_is_stretched_to_the_minimum() {
        let r = RopeEntity::new(Pos2::ZERO, 10.0);
        assert_eq!(r.length, cfg::ROPE_MIN_LENGTH);
        let loaded = Entity::from_level_entity(
            &LevelEntity::Bitmap { position: [0.0, 0.0], bitmap_name: "rope_tool".into(), size: [6.0, 1.0] },
            None,
        );
        assert_eq!(loaded.as_rope().unwrap().length, cfg::ROPE_MIN_LENGTH);
    }

    #[test]
    fn other_bitmaps_still_load_as_bitmaps() {
        let e = Entity::from_level_entity(
            &LevelEntity::Bitmap { position: [1.0, 2.0], bitmap_name: "coin_tool".into(), size: [32.0, 32.0] },
            None,
        );
        assert!(e.as_bitmap().is_some() && e.as_rope().is_none());
    }

    #[test]
    fn a_rope_is_hit_within_eight_px_of_its_line_and_moves_whole() {
        let mut rope = Entity::new_rope(Pos2::new(100.0, 100.0), 200.0);
        assert!(rope.contains_point(Pos2::new(107.0, 200.0)));
        assert!(rope.contains_point(Pos2::new(100.0, 300.0)), "the bottom end");
        assert!(!rope.contains_point(Pos2::new(110.0, 200.0)));
        assert!(!rope.contains_point(Pos2::new(100.0, 320.0)), "below the end");
        rope.translate(Vec2::new(5.0, -5.0));
        let r = rope.as_rope().unwrap();
        assert_eq!(r.anchor, Pos2::new(105.0, 95.0));
        assert_eq!(r.length, 200.0);
        assert_eq!(rope.entity_type(), "rope");
    }

    #[test]
    fn placement_takes_the_anchor_then_a_lower_click_and_ignores_the_rest() {
        let a = Pos2::new(400.0, 100.0);
        assert_eq!(RopeEntity::place(None, a), RopePlacement::SetAnchor(a));
        assert_eq!(
            RopeEntity::place(Some(a), Pos2::new(430.0, 400.0)),
            RopePlacement::Finish { anchor: a, length: 300.0 },
            "x of the second click is ignored"
        );
        assert_eq!(RopeEntity::place(Some(a), Pos2::new(400.0, 50.0)), RopePlacement::Ignore, "above");
        assert_eq!(RopeEntity::place(Some(a), Pos2::new(400.0, 150.0)), RopePlacement::Ignore, "too close");
        assert_eq!(
            RopeEntity::place(Some(a), Pos2::new(400.0, 100.0 + cfg::ROPE_MIN_LENGTH)),
            RopePlacement::Finish { anchor: a, length: cfg::ROPE_MIN_LENGTH },
            "exactly the minimum is allowed"
        );
    }

    #[test]
    fn distance_to_segment_handles_ends_and_degenerate_segments() {
        let a = Pos2::new(0.0, 0.0);
        let b = Pos2::new(0.0, 10.0);
        assert_eq!(distance_to_segment(a, b, Pos2::new(3.0, 5.0)), 3.0);
        assert_eq!(distance_to_segment(a, b, Pos2::new(0.0, 14.0)), 4.0);
        assert_eq!(distance_to_segment(a, a, Pos2::new(3.0, 4.0)), 5.0);
    }
}
