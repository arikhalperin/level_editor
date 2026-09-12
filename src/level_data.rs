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
    /// Explicit level extent [width, height] in world pixels, anchored at the top-left
    /// origin. Optional so files written before this field load unchanged.
    ///
    /// Consumers must read the level height from here rather than assuming a fixed
    /// viewport: the Bevy game currently flips editor Y with a hard-coded
    /// `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0`, which is wrong for any other height.
    #[serde(default)]
    pub level_size: Option<[f32; 2]>,
    /// All entities in the level
    pub entities: Vec<LevelEntity>,
}

impl Default for LevelData {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            background: None,
            background_size: None,
            level_size: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_size_round_trips_through_save_and_load() {
        let data = LevelData {
            version: "1.0".to_string(),
            background: None,
            background_size: Some([25600.0, 720.0]),
            level_size: Some([4000.0, 3000.0]),
            entities: vec![],
        };
        let json = serde_json::to_string(&data).expect("serialises");
        let back: LevelData = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back.level_size, Some([4000.0, 3000.0]));
        assert_eq!(back.background_size, Some([25600.0, 720.0]));
    }

    #[test]
    fn a_file_without_level_size_loads_unchanged() {
        // Exactly the shape written before this field existed.
        let json = r#"{
            "version": "1.0",
            "background": null,
            "background_size": [1920.0, 1080.0],
            "entities": []
        }"#;
        let data: LevelData = serde_json::from_str(json).expect("older files must still load");
        assert_eq!(data.level_size, None, "missing field must default to None");
        assert_eq!(data.background_size, Some([1920.0, 1080.0]));
        assert_eq!(data.version, "1.0");
    }

    #[test]
    fn a_file_with_level_size_null_loads_as_none() {
        let json = r#"{"version":"1.0","background":null,"background_size":null,
                       "level_size":null,"entities":[]}"#;
        let data: LevelData = serde_json::from_str(json).expect("null must be accepted");
        assert_eq!(data.level_size, None);
    }

    #[test]
    fn default_has_no_level_size() {
        assert_eq!(LevelData::default().level_size, None);
    }
}
