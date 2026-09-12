//! Pure play-mode camera: follow the character, and never let it leave the canvas.
//!
//! No egui `Context`/`Ui` — the caller passes the canvas panel's rectangle and an
//! explicit `dt` — so containment is unit-testable headlessly at any speed.
//!
//! Following uses the game's camera constants ([`crate::game_config`]); the containment
//! bound on top of them is the editor's own, since the game has no equivalent problem.
//!
//! Coordinates are the editor's world space: a world point `p` is drawn at screen
//! position `p - offset`.

use egui::{Pos2, Rect, Vec2};

use crate::game_config as cfg;
use crate::scroll::ScrollModel;

/// Padding kept between the character's capsule and the edge of the canvas.
///
/// An editor constant, not one of the game's: enough that the capsule never touches an
/// edge, small enough that the viewport does not feel shrunken.
pub const CAMERA_SAFE_MARGIN: f32 = 24.0;

/// Half extents of the character's capsule, which is what must stay on screen.
fn half_extents() -> Vec2 {
    Vec2::new(
        cfg::PLAYER_CAPSULE_RADIUS,
        cfg::PLAYER_CAPSULE_HALF_EXTENT_Y,
    )
}

/// Where the character appears on screen for a given scroll offset.
pub fn screen_pos(pos: Pos2, offset: Vec2) -> Pos2 {
    pos - offset
}

/// The camera's resting target: the character at the **panel's** centre, pushed below it
/// by `CAMERA_OFFSET_Y` as the game does.
///
/// Taking the centre from the panel rather than the window matters: the panel sits below
/// the menu bar, so centring on the window biases the character downward by its height.
pub fn target_offset(pos: Pos2, panel: Rect) -> Vec2 {
    let centre = panel.center();
    Vec2::new(
        pos.x - centre.x,
        pos.y - centre.y - cfg::CAMERA_OFFSET_Y,
    )
}

/// Inclusive bounds on the scroll offset that keep the capsule inside the panel's safe
/// area. When the panel is too small to hold the character plus its margins on an axis,
/// both bounds collapse to the centring value for that axis rather than inverting.
pub fn containment_bounds(pos: Pos2, panel: Rect) -> (Vec2, Vec2) {
    let half = half_extents();
    let safe = panel.shrink(CAMERA_SAFE_MARGIN);
    let mut lo = Vec2::new(pos.x - safe.max.x + half.x, pos.y - safe.max.y + half.y);
    let mut hi = Vec2::new(pos.x - safe.min.x - half.x, pos.y - safe.min.y - half.y);
    if lo.x > hi.x {
        let mid = (lo.x + hi.x) * 0.5;
        lo.x = mid;
        hi.x = mid;
    }
    if lo.y > hi.y {
        let mid = (lo.y + hi.y) * 0.5;
        lo.y = mid;
        hi.y = mid;
    }
    (lo, hi)
}

/// One frame of camera follow: converge on the target with the game's smoothing, then
/// bound the result so the character cannot be outside the safe area.
///
/// The bound is applied after the smoothing rather than by chasing faster, because only
/// a bound gives the guarantee — a quicker spring merely makes escaping less likely. It
/// binds on the frame it is needed, so no speed and no discontinuous jump in `pos` can
/// put the character off screen even briefly.
pub fn follow(offset: Vec2, pos: Pos2, vel: Vec2, panel: Rect, dt: f32) -> Vec2 {
    let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
    let target = target_offset(pos, panel);
    let mut out = offset;

    // Horizontal: ignored entirely inside the dead zone.
    let dx = target.x - out.x;
    if dx.abs() > cfg::CAMERA_DEAD_ZONE_X {
        out.x += dx * (1.0 - (-cfg::CAMERA_FOLLOW_RATE_X * dt).exp());
    }

    // Vertical: faster once the character is falling hard.
    let mut rate_y = cfg::CAMERA_FOLLOW_RATE_Y;
    if vel.y > cfg::CAMERA_FALL_SPEED_THRESHOLD {
        rate_y *= cfg::CAMERA_FALL_CATCHUP_MULT;
    }
    out.y += (target.y - out.y) * (1.0 - (-rate_y * dt).exp());

    contain(out, pos, panel)
}

/// Bound an offset so the character is inside the safe area, with no smoothing.
///
/// [`follow`] ends with this, but it is also applied on its own whenever the character
/// is moved discontinuously *after* the frame's follow has already run — placing the
/// spawn with a click does exactly that — so the character can never be drawn outside
/// the safe area, not even for the single frame before the next follow.
pub fn contain(offset: Vec2, pos: Pos2, panel: Rect) -> Vec2 {
    let (lo, hi) = containment_bounds(pos, panel);
    Vec2::new(offset.x.clamp(lo.x, hi.x), offset.y.clamp(lo.y, hi.y))
}

/// Remember the editor's view before play mode borrows the camera.
pub fn capture(scroll: &ScrollModel) -> ScrollModel {
    *scroll
}

/// Put the editor's view back, stationary: the canvas returns exactly where the user
/// left it and does not glide on from whatever momentum it had.
pub fn restore(scroll: &mut ScrollModel, saved: ScrollModel) {
    *scroll = saved;
    scroll.velocity = Vec2::ZERO;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A canvas panel below a menu bar, as the editor actually lays it out.
    fn panel() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 24.0), Vec2::new(1280.0, 696.0))
    }

    const DT: f32 = 1.0 / 60.0;

    /// The character's capsule in screen space — what containment is measured against.
    fn screen_capsule(pos: Pos2, offset: Vec2) -> Rect {
        Rect::from_center_size(screen_pos(pos, offset), half_extents() * 2.0)
    }

    fn assert_on_screen(pos: Pos2, offset: Vec2, margin: f32, label: &str) {
        let c = screen_capsule(pos, offset);
        let safe = panel().shrink(margin);
        assert!(
            c.min.x >= safe.min.x - 1e-3
                && c.max.x <= safe.max.x + 1e-3
                && c.min.y >= safe.min.y - 1e-3
                && c.max.y <= safe.max.y + 1e-3,
            "{label}: capsule {c:?} escaped the safe area {safe:?}"
        );
    }

    /// Run the character at a constant velocity, stepping the camera each frame, and
    /// assert containment throughout. Returns the final offset.
    fn run_at(vel: Vec2, secs: f32, label: &str) -> Vec2 {
        let p = panel();
        let mut pos = Pos2::new(600.0, 400.0);
        let mut offset = target_offset(pos, p);
        for _ in 0..((secs / DT) as usize) {
            pos += vel * DT;
            offset = follow(offset, pos, vel, p, DT);
            assert_on_screen(pos, offset, CAMERA_SAFE_MARGIN, label);
        }
        offset
    }

    // ── A1: never off screen ────────────────────────────────────────────────────

    #[test]
    fn the_character_stays_on_screen_at_run_speed() {
        run_at(Vec2::new(cfg::PLAYER_RUN_SPEED, 0.0), 6.0, "running right");
        run_at(Vec2::new(-cfg::PLAYER_RUN_SPEED, 0.0), 6.0, "running left");
    }

    #[test]
    fn the_character_stays_on_screen_at_dash_speed() {
        run_at(Vec2::new(cfg::DASH_SPEED, 0.0), 3.0, "dashing right");
        run_at(Vec2::new(-cfg::DASH_SPEED, 0.0), 3.0, "dashing left");
    }

    #[test]
    fn the_character_stays_on_screen_at_terminal_fall_speed() {
        run_at(Vec2::new(0.0, cfg::MAX_FALL_SPEED), 4.0, "falling");
        run_at(Vec2::new(0.0, -cfg::MAX_FALL_SPEED), 4.0, "rising");
    }

    #[test]
    fn the_character_stays_on_screen_under_knockback() {
        // A real club impulse: the shove is applied once, then gravity takes over and
        // the character accelerates downward to terminal velocity.
        let p = panel();
        let mut pos = Pos2::new(600.0, 400.0);
        let mut offset = target_offset(pos, p);
        let mut vel = Vec2::new(cfg::ORC_CLUB_KNOCKBACK_X, -cfg::ORC_CLUB_KNOCKBACK_Y);
        for _ in 0..240 {
            vel.y = (vel.y + cfg::GRAVITY * DT).min(cfg::MAX_FALL_SPEED);
            pos += vel * DT;
            offset = follow(offset, pos, vel, p, DT);
            assert_on_screen(pos, offset, CAMERA_SAFE_MARGIN, "knocked back");
        }
    }

    #[test]
    fn the_character_stays_on_screen_when_it_reverses_repeatedly() {
        let p = panel();
        let mut pos = Pos2::new(600.0, 400.0);
        let mut offset = target_offset(pos, p);
        let mut vel = Vec2::new(cfg::DASH_SPEED, 0.0);
        for frame in 0..600 {
            if frame % 20 == 0 {
                vel = -vel;
            }
            pos += vel * DT;
            offset = follow(offset, pos, vel, p, DT);
            assert_on_screen(pos, offset, CAMERA_SAFE_MARGIN, "reversing");
        }
    }

    #[test]
    fn the_clamp_is_load_bearing_when_smoothing_cannot_keep_up() {
        // Fast enough that the follow rate can never catch it: without the bound the
        // character would be left behind and off screen within a few frames.
        let p = panel();
        let mut pos = Pos2::new(600.0, 400.0);
        let mut offset = target_offset(pos, p);
        let vel = Vec2::new(120_000.0, 90_000.0);
        let mut smoothing_only = offset;
        for _ in 0..120 {
            pos += vel * DT;
            offset = follow(offset, pos, vel, p, DT);
            assert_on_screen(pos, offset, CAMERA_SAFE_MARGIN, "outrunning the camera");

            // What the smoothing alone would have produced, for contrast.
            let target = target_offset(pos, p);
            let dx = target.x - smoothing_only.x;
            if dx.abs() > cfg::CAMERA_DEAD_ZONE_X {
                smoothing_only.x += dx * (1.0 - (-cfg::CAMERA_FOLLOW_RATE_X * DT).exp());
            }
            smoothing_only.y +=
                (target.y - smoothing_only.y) * (1.0 - (-cfg::CAMERA_FOLLOW_RATE_Y * DT).exp());
        }
        let unclamped = screen_capsule(pos, smoothing_only);
        let safe = p.shrink(CAMERA_SAFE_MARGIN);
        assert!(
            unclamped.min.x < safe.min.x || unclamped.max.x > safe.max.x,
            "the test is only meaningful if smoothing alone would have lost the character"
        );
    }

    #[test]
    fn contain_is_the_bound_alone_and_is_idempotent() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let settled = target_offset(pos, p);
        // Inside the safe area it changes nothing.
        assert_eq!(contain(settled, pos, p), settled);
        // Outside, it pulls in, and applying it again is a no-op.
        let far = pos + Vec2::splat(20_000.0);
        let once = contain(settled, far, p);
        assert_on_screen(far, once, CAMERA_SAFE_MARGIN, "contain");
        assert_eq!(contain(once, far, p), once, "idempotent");
    }

    #[test]
    fn contain_catches_a_teleport_applied_after_the_frames_follow() {
        // The spawn-click case: follow runs for the old position, then the character is
        // moved, and only `contain` stands between that and the next frame's draw.
        let p = panel();
        let before = Pos2::new(600.0, 400.0);
        let offset = follow(target_offset(before, p), before, Vec2::ZERO, p, DT);
        for corner in [p.left_top(), p.right_top(), p.left_bottom(), p.right_bottom()] {
            // A click right in a canvas corner, converted to world space.
            let teleported = corner + offset;
            let fixed = contain(offset, teleported, p);
            assert_on_screen(teleported, fixed, CAMERA_SAFE_MARGIN, "corner click");
        }
    }

    // ── A2: the bound holds against a discontinuity ─────────────────────────────

    #[test]
    fn a_single_frame_teleport_cannot_escape_the_safe_area() {
        let p = panel();
        let start = Pos2::new(600.0, 400.0);
        let offset = target_offset(start, p);
        for jump in [1.0, 100.0, 5_000.0, 1_000_000.0, -50_000.0] {
            for delta in [Vec2::new(jump, 0.0), Vec2::new(0.0, jump), Vec2::splat(jump)] {
                let pos = start + delta;
                let after = follow(offset, pos, Vec2::ZERO, p, DT);
                assert_on_screen(pos, after, CAMERA_SAFE_MARGIN, &format!("teleport {delta:?}"));
            }
        }
    }

    #[test]
    fn the_rounded_offset_the_renderer_uses_is_still_on_screen() {
        // Drawing uses `ScrollModel::pixel_offset`, i.e. the offset rounded to whole
        // pixels; the margin must absorb that half-pixel.
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let offset = follow(target_offset(pos, p), pos + Vec2::splat(9_999.0), Vec2::ZERO, p, DT);
        let moved = pos + Vec2::splat(9_999.0);
        assert_on_screen(moved, offset.round(), CAMERA_SAFE_MARGIN - 1.0, "rounded");
    }

    // ── A3: the feel inside the safe area is unchanged ──────────────────────────

    #[test]
    fn the_dead_zone_holds_the_camera_still_for_small_horizontal_moves() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let settled = target_offset(pos, p);
        // Nudge the character by less than the dead zone; X must not move at all.
        let nudged = Pos2::new(pos.x + cfg::CAMERA_DEAD_ZONE_X - 1.0, pos.y);
        let after = follow(settled, nudged, Vec2::ZERO, p, DT);
        assert_eq!(after.x, settled.x, "inside the dead zone the camera must not pan");

        // Beyond it, it does move.
        let far = Pos2::new(pos.x + cfg::CAMERA_DEAD_ZONE_X + 40.0, pos.y);
        let after = follow(settled, far, Vec2::ZERO, p, DT);
        assert!(after.x > settled.x, "outside the dead zone the camera should pan");
    }

    /// An offset displaced from the resting target by `dy`, small enough that the
    /// containment bound cannot bind and the smoothing is what is being measured.
    fn offset_displaced_by(pos: Pos2, dy: f32) -> Vec2 {
        let p = panel();
        let off = target_offset(pos, p) + Vec2::new(0.0, dy);
        assert_on_screen(pos, off, CAMERA_SAFE_MARGIN, "test setup must be inside the safe area");
        off
    }

    #[test]
    fn the_vertical_follow_uses_the_games_exponential_rate() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let offset = offset_displaced_by(pos, 30.0);
        let after = follow(offset, pos, Vec2::ZERO, p, DT);
        let k = 1.0 - (-cfg::CAMERA_FOLLOW_RATE_Y * DT).exp();
        let expected = offset.y + (target_offset(pos, p).y - offset.y) * k;
        assert!(
            (after.y - expected).abs() < 0.01,
            "y follow {} should match the game's rate ({expected})",
            after.y
        );
        assert!(after.y < offset.y, "and should move toward the target");
    }

    #[test]
    fn falling_fast_speeds_the_vertical_follow() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let offset = offset_displaced_by(pos, 30.0);
        let slow = follow(offset, pos, Vec2::new(0.0, cfg::CAMERA_FALL_SPEED_THRESHOLD - 1.0), p, DT);
        let fast = follow(offset, pos, Vec2::new(0.0, cfg::CAMERA_FALL_SPEED_THRESHOLD + 1.0), p, DT);
        assert!(
            fast.y < slow.y,
            "a hard fall should close the gap faster: {} vs {}",
            fast.y,
            slow.y
        );
        let k_slow = 1.0 - (-cfg::CAMERA_FOLLOW_RATE_Y * DT).exp();
        let k_fast =
            1.0 - (-cfg::CAMERA_FOLLOW_RATE_Y * cfg::CAMERA_FALL_CATCHUP_MULT * DT).exp();
        let ratio = (fast.y - offset.y) / (slow.y - offset.y);
        assert!(
            (ratio - k_fast / k_slow).abs() < 0.01,
            "catch-up ratio {ratio} should be {}",
            k_fast / k_slow
        );
    }

    // ── A4: centred on the panel, not the window ────────────────────────────────

    #[test]
    fn the_resting_target_centres_the_character_on_the_panel() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let offset = target_offset(pos, p);
        let screen = screen_pos(pos, offset);
        assert!((screen.x - p.center().x).abs() < 1e-3, "x centred on the panel");
        assert!(
            (screen.y - (p.center().y + cfg::CAMERA_OFFSET_Y)).abs() < 1e-3,
            "y sits CAMERA_OFFSET_Y below the panel's centre"
        );
    }

    #[test]
    fn a_panel_below_a_menu_bar_is_not_treated_as_starting_at_zero() {
        let pos = Pos2::new(600.0, 400.0);
        let with_bar = Rect::from_min_size(Pos2::new(0.0, 24.0), Vec2::new(1280.0, 696.0));
        let as_window = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        assert_ne!(
            target_offset(pos, with_bar).y,
            target_offset(pos, as_window).y,
            "the menu bar must shift the target, otherwise the character is biased downward"
        );
    }

    // ── A5: past the level origin ───────────────────────────────────────────────

    #[test]
    fn the_camera_follows_the_character_past_the_level_origin() {
        let p = panel();
        // Well left of and above world zero.
        let pos = Pos2::new(-4_000.0, -3_000.0);
        let mut offset = Vec2::ZERO;
        for _ in 0..600 {
            offset = follow(offset, pos, Vec2::ZERO, p, DT);
        }
        assert!(offset.x < 0.0, "offset.x should go negative, got {}", offset.x);
        assert!(offset.y < 0.0, "offset.y should go negative, got {}", offset.y);
        assert_on_screen(pos, offset, CAMERA_SAFE_MARGIN, "past the origin");
    }

    // ── Degenerate panels ───────────────────────────────────────────────────────

    #[test]
    fn a_panel_too_small_for_the_character_centres_it_instead_of_inverting() {
        let tiny = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(40.0, 40.0));
        let pos = Pos2::new(600.0, 400.0);
        let (lo, hi) = containment_bounds(pos, tiny);
        assert_eq!(lo, hi, "the bounds must collapse rather than invert");
        let offset = follow(Vec2::ZERO, pos, Vec2::ZERO, tiny, DT);
        let screen = screen_pos(pos, offset);
        assert!(
            (screen.x - tiny.center().x).abs() < 1.0 && (screen.y - tiny.center().y).abs() < 1.0,
            "a too-small panel should centre the character, got {screen:?}"
        );
    }

    #[test]
    fn a_normal_panel_gives_a_usable_range() {
        let (lo, hi) = containment_bounds(Pos2::new(600.0, 400.0), panel());
        assert!(hi.x > lo.x && hi.y > lo.y, "bounds should not be degenerate: {lo:?}..{hi:?}");
    }

    #[test]
    fn follow_ignores_invalid_dt_but_still_contains() {
        let p = panel();
        let pos = Pos2::new(600.0, 400.0);
        let settled = target_offset(pos, p);
        for bad in [f32::NAN, -1.0, 0.0] {
            assert_eq!(follow(settled, pos, Vec2::ZERO, p, bad), settled, "dt {bad}");
        }
        // Even with no time passing the bound still applies to a moved character.
        let far = pos + Vec2::splat(10_000.0);
        assert_on_screen(far, follow(settled, far, Vec2::ZERO, p, 0.0), CAMERA_SAFE_MARGIN, "dt 0");
    }

    // ── A7, A8: remembering the editor's view ───────────────────────────────────

    #[test]
    fn capture_and_restore_return_the_exact_offset_without_momentum() {
        let mut scroll = ScrollModel {
            offset: Vec2::new(1234.5, 678.25),
            velocity: Vec2::new(-300.0, 90.0),
        };
        let saved = capture(&scroll);

        // Play mode drags the camera somewhere else entirely, including negative.
        scroll.offset = Vec2::new(-9_000.0, -7_500.0);
        scroll.velocity = Vec2::new(42.0, 42.0);

        restore(&mut scroll, saved);
        assert_eq!(scroll.offset, Vec2::new(1234.5, 678.25), "exact offset back");
        assert_eq!(scroll.velocity, Vec2::ZERO, "and stationary");
    }

    #[test]
    fn capture_does_not_alias_the_live_model() {
        let mut scroll = ScrollModel { offset: Vec2::new(10.0, 20.0), velocity: Vec2::ZERO };
        let saved = capture(&scroll);
        scroll.offset = Vec2::new(999.0, 999.0);
        assert_eq!(saved.offset, Vec2::new(10.0, 20.0), "the snapshot must be a copy");
    }
}
