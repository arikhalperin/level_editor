//! What one undoable step restores.
//!
//! Undo used to hold the entity list alone, which was all any edit touched. Generating a
//! level replaces the level size and its start and end as well, so a snapshot carries
//! those too and one Ctrl+Z puts the whole level back. For an ordinary edit the extra
//! fields are simply unchanged, so nothing about undo's existing behaviour moves.

use std::path::PathBuf;

use egui::{Pos2, Vec2};

use crate::entities::Entity;

/// Everything an undoable step restores.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub entities: Vec<Entity>,
    pub level_size: Option<Vec2>,
    pub level_spawn: Option<Pos2>,
    pub level_exit: Option<Pos2>,
    /// Which file this level came from. Generation deliberately clears it, so without it
    /// here an undo would give the old level back with its file association lost and the
    /// next save would ask for a destination instead of overwriting.
    pub last_level_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_snapshot_is_an_empty_level_with_nothing_set() {
        let snapshot = Snapshot::default();
        assert!(snapshot.entities.is_empty());
        assert_eq!(snapshot.level_size, None);
        assert_eq!(snapshot.level_spawn, None);
        assert_eq!(snapshot.level_exit, None);
        assert_eq!(snapshot.last_level_path, None);
    }
}
