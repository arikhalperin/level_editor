//! Starting a level from scratch: the blank slate, and the "is this level dirty?" hash.
//!
//! Pure — no egui `Context`/`Ui` and no editor type — so the reset's contents and the
//! clean/dirty marker can be checked headlessly. `main.rs` assigns these values onto its
//! own state and owns the menu item and the confirmation dialog.

use std::path::PathBuf;

use egui::{Pos2, Vec2};

use crate::entities::Entity;
use crate::scroll::ScrollModel;

/// Everything `File → New Level` resets, gathered in one place so the reset cannot
/// silently omit a field.
#[derive(Debug, Default)]
pub struct BlankLevel {
    pub entities: Vec<Entity>,
    /// No explicit size: the extent falls back through the resolution order.
    pub level_size: Option<Vec2>,
    /// No background image loaded, so no recorded background size.
    pub background_size: Vec2,
    pub play_spawn: Option<Pos2>,
    pub selected_entity: Option<usize>,
    pub editing_polygon_entity: Option<usize>,
    pub editing_polygon_point: Option<usize>,
    pub undo_stack: Vec<Vec<Entity>>,
    pub redo_stack: Vec<Vec<Entity>>,
    /// The view returns to the origin, stationary.
    pub scroll: ScrollModel,
    /// Forgotten, so the next save asks where to put the file instead of overwriting
    /// the level that was previously open.
    pub last_level_path: Option<PathBuf>,
    pub last_background_path: Option<PathBuf>,
}

impl BlankLevel {
    pub fn new() -> Self {
        Self::default()
    }

    /// The hash a freshly created level should be marked clean with. Taken from the
    /// blank content itself rather than written out separately, so the clean marker and
    /// the contents cannot disagree.
    pub fn clean_hash(&self) -> u64 {
        level_hash(&self.entities, self.level_size)
    }
}

/// Fingerprint of everything that is saved with a level, for the unsaved-changes check.
pub fn level_hash(entities: &[Entity], level_size: Option<Vec2>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    format!("{entities:?}").hash(&mut hasher);
    // The explicit level size is saved with the level, so changing it is an unsaved
    // change too.
    format!("{level_size:?}").hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Entity;

    fn a_polygon() -> Entity {
        Entity::new_polygon_with_type_and_color(
            vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0)],
            "wall_tool".to_string(),
            None,
        )
    }

    #[test]
    fn a_blank_level_has_nothing_in_it() {
        let b = BlankLevel::new();
        assert!(b.entities.is_empty(), "no entities");
        assert_eq!(b.level_size, None, "no explicit size");
        assert_eq!(b.background_size, Vec2::ZERO, "no background");
        assert_eq!(b.play_spawn, None, "no spawn point");
        assert_eq!(b.selected_entity, None, "nothing selected");
        assert_eq!(b.editing_polygon_entity, None, "no polygon being edited");
        assert_eq!(b.editing_polygon_point, None);
        assert!(b.undo_stack.is_empty() && b.redo_stack.is_empty(), "no history");
        assert_eq!(b.last_level_path, None, "no remembered level path");
        assert_eq!(b.last_background_path, None, "no remembered background path");
    }

    #[test]
    fn a_blank_level_puts_the_view_back_at_the_origin() {
        let b = BlankLevel::new();
        assert_eq!(b.scroll.offset, Vec2::ZERO, "view at the origin");
        assert_eq!(b.scroll.velocity, Vec2::ZERO, "and stationary");
    }

    #[test]
    fn a_blank_levels_clean_hash_matches_its_own_contents() {
        // The property that matters: the marker the editor records as "saved" is the
        // hash of what the new level actually contains, so it cannot start out dirty.
        let b = BlankLevel::new();
        assert_eq!(b.clean_hash(), level_hash(&b.entities, b.level_size));
        assert_eq!(b.clean_hash(), level_hash(&[], None));
        // And it is a marker of *this* content, not a constant that reads clean for
        // anything: the level that was open before the reset does not match it.
        assert_ne!(
            b.clean_hash(),
            level_hash(&[a_polygon()], Some(Vec2::new(4000.0, 3000.0))),
            "the previous level must not hash as the blank one"
        );
    }

    #[test]
    fn the_hash_notices_entities_and_the_level_size() {
        let blank = level_hash(&[], None);
        assert_ne!(level_hash(&[a_polygon()], None), blank, "an entity is a change");
        assert_ne!(
            level_hash(&[], Some(Vec2::new(4000.0, 3000.0))),
            blank,
            "setting the level size is a change"
        );
        assert_eq!(level_hash(&[], None), blank, "and an identical level hashes the same");
    }
}
