use serde::{Deserialize, Serialize};

/// Level data format for Bevy 2D platformer games
/// Colliders and physics properties are configured in game code based on entity types
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LevelData {
    /// Format version for future compatibility
    pub version: String,
    /// Background image path (relative to game assets)
    pub background: Option<String>,
    /// Background dimensions
    pub background_size: Option<[f32; 2]>,
    /// All entities in the level
    pub entities: Vec<LevelEntity>,
}

impl Default for LevelData {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            background: None,
            background_size: None,
            entities: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum LevelEntity {
    #[serde(rename = "bitmap")]
    Bitmap {
        /// Position in world space
        position: [f32; 2],
        /// Asset name or path (e.g., "platform", "coin", "enemy")
        bitmap_name: String,
        /// Size of the bitmap
        size: [f32; 2],
    },
    #[serde(rename = "polygon")]
    Polygon {
        /// Vertices of the polygon in world space
        vertices: Vec<[f32; 2]>,
        /// Polygon type (e.g., "wall", "blocker", "polygon")
        #[serde(default)]
        polygon_type: Option<String>,
        /// Optional color in hex format (e.g., "#FF0000")
        #[serde(default)]
        color: Option<String>,
    },
}
