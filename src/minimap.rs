//! Pure minimap geometry: where the navigation box sits on screen and how a point on
//! it maps back to a view offset.
//!
//! No egui `Context` or `Ui` dependency — only the plain `Rect` / `Pos2` / `Vec2` math
//! types — so the renderer and the mouse hit-test share one layout and the mapping is
//! unit-testable.

use egui::{Pos2, Rect, Vec2};

/// Gap between the minimap and the panel's right / bottom edges.
pub const MARGIN: f32 = 16.0;
/// Longest edge of the minimap before the minimum-side rule.
pub const MAX_DIM: f32 = 220.0;
/// Wide levels: the initial scale can make one side a hairline; enforce a readable size.
pub const MIN_SIDE: f32 = 100.0;
/// Width of the frame between the outer box and the thumbnail.
pub const FRAME: f32 = 4.0;

/// The minimap's on-screen geometry for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapLayout {
    /// The whole box, frame included. This is the clickable area.
    pub map_rect: Rect,
    /// The thumbnail area inside the frame; maps linearly onto the level.
    pub inner: Rect,
    /// Level extent the thumbnail represents (world pixels).
    pub level: Vec2,
}

/// Compute the minimap's rectangle for a panel of `panel` and a level extent of `level`,
/// clipped to `screen`. Returns `None` when the panel is too small to host one (the
/// renderer then draws nothing and nothing is clickable).
///
/// `level` components below 1 fall back to the panel size so the box still makes sense on
/// an empty level.
pub fn layout(panel: Rect, screen: Rect, level: Vec2) -> Option<MinimapLayout> {
    if panel.width() < 32.0 || panel.height() < 32.0 {
        return None;
    }
    let level = if level.x >= 1.0 && level.y >= 1.0 {
        level
    } else {
        Vec2::new(panel.width().max(1.0), panel.height().max(1.0))
    };

    let avail_w = (panel.width() - MARGIN * 2.0).max(1.0);
    let avail_h = (panel.height() - MARGIN * 2.0).max(1.0);
    let lw = level.x.max(1.0);
    let lh = level.y.max(1.0);
    let scale = (avail_w / lw).min(avail_h / lh).min(MAX_DIM / lw.max(lh));
    let mut map_w = lw * scale;
    let mut map_h = lh * scale;
    let min_side = map_w.min(map_h);
    if min_side < MIN_SIDE {
        let factor = MIN_SIDE / min_side;
        map_w *= factor;
        map_h *= factor;
    }
    let clamp = (avail_w / map_w).min(avail_h / map_h).min(1.0);
    map_w *= clamp;
    map_h *= clamp;
    let map_pos = Pos2::new(panel.right() - MARGIN - map_w, panel.bottom() - MARGIN - map_h);
    let map_rect = Rect::from_min_size(map_pos, Vec2::new(map_w, map_h)).intersect(screen);
    // Too small to hold a thumbnail inside its frame: draw nothing, click nothing.
    if map_rect.width() <= FRAME * 2.0 + 1.0 || map_rect.height() <= FRAME * 2.0 + 1.0 {
        return None;
    }
    Some(MinimapLayout { map_rect, inner: map_rect.shrink(FRAME), level })
}

impl MinimapLayout {
    /// True when a pointer at `p` is over the box, frame included.
    pub fn contains(&self, p: Pos2) -> bool {
        self.map_rect.contains(p)
    }

    /// The world point under screen position `p`. Positions outside the thumbnail are
    /// clamped to its edge first, so the frame — and a drag that has wandered off the
    /// box — map to the nearest level edge.
    pub fn world_at(&self, p: Pos2) -> Pos2 {
        let p = self.inner.clamp(p);
        let fx = (p.x - self.inner.min.x) / self.inner.width().max(f32::EPSILON);
        let fy = (p.y - self.inner.min.y) / self.inner.height().max(f32::EPSILON);
        Pos2::new(fx * self.level.x, fy * self.level.y)
    }

    /// The scroll offset that centres a viewport of `panel_size` on screen position `p`,
    /// clamped to the same reachable range arrow-key scrolling uses (`0 ..= level`).
    pub fn offset_for_pointer(&self, p: Pos2, panel_size: Vec2) -> Vec2 {
        centre_offset(self.world_at(p), panel_size, self.level)
    }
}

/// The scroll offset that puts `world` at the centre of a viewport of `panel_size`,
/// clamped to `0 ..= level` per axis so the view never leaves the reachable range.
pub fn centre_offset(world: Pos2, panel_size: Vec2, level: Vec2) -> Vec2 {
    let raw = world.to_vec2() - panel_size * 0.5;
    Vec2::new(
        raw.x.clamp(0.0, level.x.max(0.0)),
        raw.y.clamp(0.0, level.y.max(0.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 760.0));
    /// A central panel below a 40 px menu bar.
    const PANEL: Rect = Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(1280.0, 760.0));
    /// The bundled level_image.png.
    const WIDE: Vec2 = Vec2::new(25600.0, 720.0);

    fn wide() -> MinimapLayout {
        layout(PANEL, SCREEN, WIDE).expect("a wide level gets a minimap")
    }

    #[test]
    fn the_box_sits_in_the_panels_bottom_right_corner() {
        let l = wide();
        assert!((l.map_rect.right() - (PANEL.right() - MARGIN)).abs() < 1e-3);
        assert!((l.map_rect.bottom() - (PANEL.bottom() - MARGIN)).abs() < 1e-3);
        assert_eq!(l.inner, l.map_rect.shrink(FRAME), "the thumbnail sits inside the frame");
        assert_eq!(l.level, WIDE);
    }

    #[test]
    fn a_wide_level_is_as_readable_as_the_panel_allows() {
        // 25600×720 at the 220 px rule would be a 6 px hairline; the minimum-side rule
        // scales it up and the panel-fit clamp then brings it back to the widest box that
        // fits, which is still far taller than the hairline.
        let l = wide();
        let hairline = MAX_DIM * WIDE.y / WIDE.x;
        assert!(l.map_rect.height() > hairline * 3.0, "got {}", l.map_rect.height());
        assert!(l.map_rect.width() > l.map_rect.height(), "and stays wide");
        assert!((l.map_rect.width() - (PANEL.width() - MARGIN * 2.0)).abs() < 1e-2, "filling the panel width");
    }

    #[test]
    fn a_tiny_panel_has_no_minimap() {
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(20.0, 20.0));
        assert_eq!(layout(tiny, SCREEN, WIDE), None);
    }

    #[test]
    fn a_box_clipped_thinner_than_its_frame_is_dropped() {
        // A screen that cuts the box down to a sliver would invert `inner`; there is no
        // minimap then rather than a degenerate one.
        let full = layout(PANEL, SCREEN, WIDE).unwrap();
        // A screen ending 6 px past the box's left edge leaves a 6 px sliver.
        let sliver = Rect::from_min_max(Pos2::ZERO, Pos2::new(full.map_rect.left() + 6.0, 760.0));
        assert_eq!(layout(PANEL, sliver, WIDE), None);
        let ok = Rect::from_min_max(Pos2::ZERO, Pos2::new(full.map_rect.left() + 40.0, 760.0));
        let l = layout(PANEL, ok, WIDE).expect("a clipped but usable box");
        assert!(l.inner.width() > 0.0 && l.inner.height() > 0.0);
        assert!(l.map_rect.width() <= 40.0 + 1e-3);
    }

    #[test]
    fn an_empty_level_falls_back_to_the_panel_shape() {
        let l = layout(PANEL, SCREEN, Vec2::ZERO).expect("still drawn");
        assert_eq!(l.level, PANEL.size());
    }

    #[test]
    fn contains_covers_the_frame_too() {
        let l = wide();
        assert!(l.contains(l.map_rect.min + Vec2::new(1.0, 1.0)), "on the frame");
        assert!(l.contains(l.inner.center()));
        assert!(!l.contains(l.map_rect.min - Vec2::new(1.0, 1.0)), "just outside");
    }

    #[test]
    fn the_middle_of_the_thumbnail_is_the_middle_of_the_level() {
        let l = wide();
        let w = l.world_at(l.inner.center());
        assert!((w.x - WIDE.x / 2.0).abs() < 0.5, "got {}", w.x);
        assert!((w.y - WIDE.y / 2.0).abs() < 0.5, "got {}", w.y);
    }

    #[test]
    fn points_off_the_thumbnail_clamp_to_the_nearest_edge() {
        let l = wide();
        assert_eq!(l.world_at(Pos2::new(-500.0, -500.0)), Pos2::ZERO);
        let far = l.world_at(Pos2::new(5000.0, 5000.0));
        assert!((far.x - WIDE.x).abs() < 1e-2 && (far.y - WIDE.y).abs() < 1e-2);
        // The frame maps to the edge as well.
        let frame = l.world_at(l.map_rect.min);
        assert_eq!(frame, Pos2::ZERO);
    }

    // ── Centring ─────────────────────────────────────────────────────────────

    #[test]
    fn a_press_in_the_middle_centres_the_view_on_the_level_middle() {
        let l = wide();
        let off = l.offset_for_pointer(l.inner.center(), PANEL.size());
        let expect_x = WIDE.x / 2.0 - PANEL.width() / 2.0;
        assert!((off.x - expect_x).abs() < 1.0, "x {} vs {}", off.x, expect_x);
        // The level is shorter than the panel, so vertically the origin is the nearest
        // reachable centring.
        assert_eq!(off.y, 0.0);
    }

    #[test]
    fn the_offset_never_goes_negative_near_the_origin() {
        // A target centre less than half a viewport from the origin pins the view at 0.
        let panel = Vec2::new(1280.0, 720.0);
        let off = centre_offset(Pos2::new(100.0, 50.0), panel, WIDE);
        assert_eq!(off, Vec2::ZERO);
        // Exactly half a viewport in is the first spot that does not clamp.
        let off = centre_offset(Pos2::new(640.0, 360.0), panel, WIDE);
        assert_eq!(off, Vec2::ZERO);
        let off = centre_offset(Pos2::new(641.0, 361.0), panel, WIDE);
        assert!(off.x > 0.0 && off.y > 0.0);
    }

    #[test]
    fn the_offset_never_passes_the_level_size_near_the_far_edge() {
        let panel = Vec2::new(1280.0, 720.0);
        let level = Vec2::new(4000.0, 3000.0);
        let off = centre_offset(Pos2::new(4000.0, 3000.0), panel, level);
        // Unclamped this would be level − panel/2, well inside the range; the bound is
        // only hit when the target is far past the level.
        assert_eq!(off, level - panel * 0.5);
        let off = centre_offset(Pos2::new(9000.0, 9000.0), panel, level);
        assert_eq!(off, level, "pinned to the reachable bound, never beyond");
    }

    #[test]
    fn dragging_across_the_thumbnail_sweeps_the_whole_level() {
        let l = wide();
        let left = l.offset_for_pointer(Pos2::new(l.inner.left(), l.inner.center().y), PANEL.size());
        let right = l.offset_for_pointer(Pos2::new(l.inner.right(), l.inner.center().y), PANEL.size());
        assert_eq!(left.x, 0.0, "the origin is the nearest reachable centring of the left edge");
        // Centring on the far edge puts that edge in the middle of the view, well inside
        // the reachable range.
        let far = WIDE.x - PANEL.width() / 2.0;
        assert!((right.x - far).abs() < 1e-2, "far edge centred, got {} vs {}", right.x, far);
        assert!(right.x <= WIDE.x, "and never past the bound");
        // Monotonic in between.
        let mut prev = left.x;
        for i in 1..=10 {
            let t = i as f32 / 10.0;
            let x = l.inner.left() + t * l.inner.width();
            let o = l.offset_for_pointer(Pos2::new(x, l.inner.center().y), PANEL.size());
            assert!(o.x >= prev, "offset must not go backwards while dragging right");
            prev = o.x;
        }
    }

    #[test]
    fn a_degenerate_level_never_inverts_the_range() {
        let off = centre_offset(Pos2::new(50.0, 50.0), Vec2::new(800.0, 600.0), Vec2::new(-10.0, -10.0));
        assert_eq!(off, Vec2::ZERO);
    }
}
