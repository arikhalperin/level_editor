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
    /// Where a run of this level begins, in world pixels. Optional and defaulted exactly
    /// as `level_size` is, so a file written before this field loads unchanged: a level
    /// placed by hand simply has no spawn and play mode keeps starting the character at
    /// the centre of the visible canvas.
    #[serde(default)]
    pub spawn: Option<[f32; 2]>,
    /// Where the critical path ends, in world pixels. Written by AI generation so the
    /// route it proved can be re-proved later; absent in every hand-built level.
    #[serde(default)]
    pub exit: Option<[f32; 2]>,
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
            spawn: None,
            exit: None,
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
        /// Optional path to an image tiled across the polygon. Absent in files written
        /// before patterns existed, which therefore load with no pattern.
        #[serde(default)]
        pattern: Option<String>,
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
            spawn: None,
            exit: None,
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

    #[test]
    fn a_polygon_pattern_survives_the_round_trip() {
        let data = LevelData {
            entities: vec![LevelEntity::Polygon {
                vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                polygon_type: Some("blocker_tool".to_string()),
                color: Some("#FFFF00".to_string()),
                pattern: Some("/tmp/bricks.png".to_string()),
            }],
            ..LevelData::default()
        };

        let json = serde_json::to_string(&data).expect("serialises");
        let back: LevelData = serde_json::from_str(&json).expect("deserialises");

        match &back.entities[0] {
            LevelEntity::Polygon { pattern, color, polygon_type, .. } => {
                assert_eq!(pattern.as_deref(), Some("/tmp/bricks.png"), "the pattern came back");
                assert_eq!(color.as_deref(), Some("#FFFF00"), "and did not disturb the colour");
                assert_eq!(polygon_type.as_deref(), Some("blocker_tool"));
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn a_polygon_written_before_patterns_existed_loads_with_none() {
        // Exactly the shape written before this field existed: no `pattern` key at all.
        let json = r##"{
            "version": "1.0",
            "background": null,
            "background_size": null,
            "entities": [
                {
                    "type": "polygon",
                    "vertices": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                    "polygon_type": "blocker_tool",
                    "color": "#FFFF00"
                }
            ]
        }"##;
        let data: LevelData = serde_json::from_str(json).expect("older files must still load");
        match &data.entities[0] {
            LevelEntity::Polygon { pattern, color, .. } => {
                assert_eq!(*pattern, None, "a missing key means no pattern");
                assert_eq!(color.as_deref(), Some("#FFFF00"), "everything else is unaffected");
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn an_explicit_null_pattern_loads_as_none() {
        let json = r#"{"version":"1.0","background":null,"background_size":null,"entities":[
            {"type":"polygon","vertices":[[0.0,0.0],[1.0,0.0],[1.0,1.0]],
             "polygon_type":"blocker_tool","color":null,"pattern":null}]}"#;
        let data: LevelData = serde_json::from_str(json).expect("null must be accepted");
        match &data.entities[0] {
            LevelEntity::Polygon { pattern, .. } => assert_eq!(*pattern, None),
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn spawn_and_exit_round_trip_through_save_and_load() {
        let data = LevelData {
            spawn: Some([1200.0, 700.0]),
            exit: Some([5200.0, 2400.0]),
            ..LevelData::default()
        };
        let json = serde_json::to_string(&data).expect("serialises");
        let back: LevelData = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back.spawn, Some([1200.0, 700.0]), "the spawn came back");
        assert_eq!(back.exit, Some([5200.0, 2400.0]), "and so did the exit");
    }

    #[test]
    fn a_file_written_before_spawn_and_exit_existed_loads_unchanged() {
        // Exactly the shape written before these fields existed: no keys at all.
        let json = r#"{
            "version": "1.0",
            "background": null,
            "background_size": [1920.0, 1080.0],
            "level_size": [4000.0, 3000.0],
            "entities": []
        }"#;
        let data: LevelData = serde_json::from_str(json).expect("older files must still load");
        assert_eq!(data.spawn, None, "a missing key means no spawn");
        assert_eq!(data.exit, None, "a missing key means no exit");
        assert_eq!(data.level_size, Some([4000.0, 3000.0]), "and nothing else is disturbed");
    }

    #[test]
    fn an_explicit_null_spawn_or_exit_loads_as_none() {
        let json = r#"{"version":"1.0","background":null,"background_size":null,
                       "level_size":null,"spawn":null,"exit":null,"entities":[]}"#;
        let data: LevelData = serde_json::from_str(json).expect("null must be accepted");
        assert_eq!(data.spawn, None);
        assert_eq!(data.exit, None);
    }

    #[test]
    fn default_has_no_spawn_or_exit() {
        assert_eq!(LevelData::default().spawn, None);
        assert_eq!(LevelData::default().exit, None);
    }
}
