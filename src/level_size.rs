//! The level's explicit extent in world pixels.
//!
//! Pure model: resolution order, validation and the boundary rectangle. No egui
//! `Context`/`Ui`, so it is unit-testable headlessly. `main.rs` owns the dialog.

use egui::{Pos2, Rect, Vec2};

/// Used when a level has no explicit size, no background and no entities.
pub const DEFAULT_LEVEL_SIZE: Vec2 = Vec2::new(1920.0, 1080.0);
/// Smallest accepted edge, in pixels.
pub const MIN_LEVEL_EDGE: f32 = 1.0;
/// Largest accepted edge, in pixels.
pub const MAX_LEVEL_EDGE: f32 = 1_000_000.0;

/// Where a resolved size came from. Useful in the dialog and for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeSource {
    Explicit,
    Background,
    EntityBounds,
    Default,
}

/// True when both components are finite and within the accepted range.
pub fn is_valid_size(size: Vec2) -> bool {
    let ok = |v: f32| v.is_finite() && (MIN_LEVEL_EDGE..=MAX_LEVEL_EDGE).contains(&v);
    ok(size.x) && ok(size.y)
}

/// Validate a candidate size, returning `None` when it must be refused.
pub fn validate(size: Vec2) -> Option<Vec2> {
    is_valid_size(size).then_some(size)
}

/// Resolve the level extent: explicit size, else background size, else the bounding box
/// of all entity extents, else [`DEFAULT_LEVEL_SIZE`].
///
/// `entity_bounds` is the maximum (x, y) reached by any entity, or `None` when the level
/// is empty. Only valid candidates are accepted at each step.
pub fn resolve(
    explicit: Option<Vec2>,
    background: Option<Vec2>,
    entity_bounds: Option<Vec2>,
) -> (Vec2, SizeSource) {
    if let Some(v) = explicit.and_then(validate) {
        return (v, SizeSource::Explicit);
    }
    if let Some(v) = background.and_then(validate) {
        return (v, SizeSource::Background);
    }
    if let Some(v) = entity_bounds.and_then(validate) {
        return (v, SizeSource::EntityBounds);
    }
    (DEFAULT_LEVEL_SIZE, SizeSource::Default)
}

/// The boundary rectangle in world space: (0, 0) to (width, height).
pub fn boundary_rect(size: Vec2) -> Rect {
    Rect::from_min_size(Pos2::ZERO, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: Vec2 = Vec2::new(25600.0, 720.0);
    const ENT: Vec2 = Vec2::new(800.0, 600.0);
    const EXP: Vec2 = Vec2::new(4000.0, 3000.0);

    #[test]
    fn resolution_order_prefers_explicit_then_background_then_entities_then_default() {
        assert_eq!(resolve(Some(EXP), Some(BG), Some(ENT)), (EXP, SizeSource::Explicit));
        assert_eq!(resolve(None, Some(BG), Some(ENT)), (BG, SizeSource::Background));
        assert_eq!(resolve(None, None, Some(ENT)), (ENT, SizeSource::EntityBounds));
        assert_eq!(resolve(None, None, None), (DEFAULT_LEVEL_SIZE, SizeSource::Default));
    }

    #[test]
    fn an_invalid_candidate_falls_through_to_the_next_source() {
        let bad = Vec2::new(0.0, 100.0);
        assert_eq!(resolve(Some(bad), Some(BG), None), (BG, SizeSource::Background));
        assert_eq!(resolve(Some(bad), Some(bad), Some(ENT)), (ENT, SizeSource::EntityBounds));
        assert_eq!(
            resolve(Some(bad), Some(bad), Some(bad)),
            (DEFAULT_LEVEL_SIZE, SizeSource::Default)
        );
    }

    #[test]
    fn rejects_non_finite_zero_negative_and_oversized_values() {
        for bad in [
            Vec2::new(f32::NAN, 100.0),
            Vec2::new(100.0, f32::NAN),
            Vec2::new(f32::INFINITY, 100.0),
            Vec2::new(100.0, f32::NEG_INFINITY),
            Vec2::new(0.0, 100.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(-1.0, 100.0),
            Vec2::new(100.0, -1.0),
            Vec2::new(MAX_LEVEL_EDGE + 1.0, 100.0),
            Vec2::new(100.0, MAX_LEVEL_EDGE + 1.0),
        ] {
            assert!(!is_valid_size(bad), "{bad:?} must be refused");
            assert_eq!(validate(bad), None);
        }
    }

    #[test]
    fn accepts_the_range_boundaries() {
        for good in [
            Vec2::new(MIN_LEVEL_EDGE, MIN_LEVEL_EDGE),
            Vec2::new(MAX_LEVEL_EDGE, MAX_LEVEL_EDGE),
            Vec2::new(1920.0, 1080.0),
        ] {
            assert!(is_valid_size(good), "{good:?} must be accepted");
            assert_eq!(validate(good), Some(good));
        }
    }

    #[test]
    fn boundary_rect_runs_from_the_origin_to_the_size() {
        let r = boundary_rect(Vec2::new(4000.0, 3000.0));
        assert_eq!(r.min, Pos2::ZERO);
        assert_eq!(r.max, Pos2::new(4000.0, 3000.0));
    }
}
