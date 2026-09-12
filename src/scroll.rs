//! Pure canvas scroll model: two-axis arrow-key scrolling with momentum.
//!
//! This module has no dependency on egui's `Context`, `Ui`, or input system;
//! it only uses the plain `Vec2` math type. The model is advanced once per
//! frame with an explicit `dt`, which keeps it deterministic and unit-testable.

use egui::Vec2;

/// Maximum scroll speed in pixels per second (previously a fixed 600 px/s).
pub const MAX_SPEED: f32 = 1200.0;
/// Acceleration toward `MAX_SPEED` while a key is held, in px/s².
/// `MAX_SPEED / ACCEL` = 0.25 s to reach full speed.
pub const ACCEL: f32 = 4800.0;
/// Exponential decay rate applied on release, in 1/s.
/// From `MAX_SPEED` down to `STOP_SPEED` takes `ln(1200/5)/6 ≈ 0.91 s` (< 1.5 s).
pub const DECAY: f32 = 6.0;
/// Speed below which momentum snaps to exactly zero, in px/s.
pub const STOP_SPEED: f32 = 5.0;

/// Which arrow directions are currently held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldDirs {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl HeldDirs {
    /// True if any arrow key is held.
    pub fn any(&self) -> bool {
        self.left || self.right || self.up || self.down
    }

    /// Net direction per axis in {-1, 0, +1}. Opposite keys cancel.
    fn axis_input(&self) -> Vec2 {
        let x = (self.right as i8 - self.left as i8) as f32;
        let y = (self.down as i8 - self.up as i8) as f32;
        Vec2::new(x, y)
    }
}

/// Continuous scroll state: world position at the viewport's top-left plus velocity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollModel {
    /// World offset; never negative on either axis, no upper bound.
    pub offset: Vec2,
    /// Current velocity in px/s.
    pub velocity: Vec2,
}

impl ScrollModel {
    /// Advance the model by `dt` seconds with the given held directions.
    /// Advance the model by `dt`, confined to `0 ..= max` on each axis.
    ///
    /// `max` is supplied every step so the reachable area tracks the level size live: the
    /// editor passes the resolved level extent, which puts one full viewport of slack
    /// past the level's far edge (the visible region is `offset ..= offset + viewport`).
    /// A negative component is treated as zero so the range can never invert.
    pub fn step(&mut self, held: HeldDirs, dt: f32, max: Vec2) {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        let dir = held.axis_input();
        Self::step_axis(&mut self.offset.x, &mut self.velocity.x, dir.x, dt, max.x);
        Self::step_axis(&mut self.offset.y, &mut self.velocity.y, dir.y, dt, max.y);
    }

    fn step_axis(pos: &mut f32, vel: &mut f32, dir: f32, dt: f32, max: f32) {
        if dir != 0.0 {
            // Accelerate toward the target speed at a fixed rate.
            let target = dir * MAX_SPEED;
            let dv = ACCEL * dt;
            if *vel < target {
                *vel = (*vel + dv).min(target);
            } else if *vel > target {
                *vel = (*vel - dv).max(target);
            }
        } else {
            // Momentum: exponential decay, snapping to an exact stop.
            *vel *= (-DECAY * dt).exp();
            if vel.abs() < STOP_SPEED {
                *vel = 0.0;
            }
        }

        *pos += *vel * dt;

        // Confine to the reachable range. Momentum stops dead at either end rather than
        // overshooting or bouncing.
        let max = if max.is_finite() { max.max(0.0) } else { 0.0 };
        if *pos < 0.0 {
            *pos = 0.0;
            *vel = 0.0;
        } else if *pos > max {
            *pos = max;
            *vel = 0.0;
        }
    }

    /// True while either axis still has velocity.
    pub fn is_moving(&self) -> bool {
        self.velocity != Vec2::ZERO
    }

    /// True when the app must repaint next frame to keep the motion advancing.
    pub fn needs_repaint(&self, held: HeldDirs) -> bool {
        held.any() || self.is_moving()
    }

    /// The single integer-pixel offset shared by every renderer this frame.
    pub fn pixel_offset(&self) -> Vec2 {
        self.offset.round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;
    /// Far beyond anything the behaviour tests reach, so they exercise motion rather than
    /// the bound. The bound has its own tests below.
    const FAR: Vec2 = Vec2::new(1.0e9, 1.0e9);

    fn hold(dirs: HeldDirs, secs: f32) -> ScrollModel {
        let mut m = ScrollModel::default();
        run(&mut m, dirs, secs);
        m
    }

    fn run(m: &mut ScrollModel, dirs: HeldDirs, secs: f32) {
        let steps = (secs / DT).round() as usize;
        for _ in 0..steps {
            m.step(dirs, DT, FAR);
        }
    }

    const RIGHT: HeldDirs = HeldDirs { right: true, ..NONE };
    const LEFT: HeldDirs = HeldDirs { left: true, ..NONE };
    const DOWN: HeldDirs = HeldDirs { down: true, ..NONE };
    const UP: HeldDirs = HeldDirs { up: true, ..NONE };
    const NONE: HeldDirs = HeldDirs { left: false, right: false, up: false, down: false };

    #[test]
    fn arrow_down_increases_y_and_arrow_up_decreases_it() {
        let mut m = hold(DOWN, 1.0);
        assert!(m.offset.y > 0.0, "ArrowDown must increase scroll_offset.y");
        assert_eq!(m.offset.x, 0.0, "ArrowDown must not move x");
        let y_after_down = m.offset.y;
        run(&mut m, UP, 0.5);
        assert!(m.offset.y < y_after_down, "ArrowUp must decrease scroll_offset.y");
    }

    #[test]
    fn arrow_right_increases_x_and_arrow_left_decreases_it() {
        let mut m = hold(RIGHT, 1.0);
        assert!(m.offset.x > 0.0, "ArrowRight must increase scroll_offset.x");
        assert_eq!(m.offset.y, 0.0, "ArrowRight must not move y");
        let x_after_right = m.offset.x;
        run(&mut m, LEFT, 0.5);
        assert!(m.offset.x < x_after_right, "ArrowLeft must decrease scroll_offset.x");
    }

    #[test]
    fn scrolling_reaches_past_the_background_when_the_level_is_that_big() {
        // The bundled level_image.png is 25600 x 720, and a level sized to it can be
        // scrolled all the way to its far edge — a full viewport past the image.
        let level = Vec2::new(25600.0, 720.0);
        let mut m = ScrollModel::default();
        run_bounded(&mut m, RIGHT, 60.0, level);
        assert!(m.offset.x > 25600.0 - 1.0, "x should reach the level's far edge, got {}", m.offset.x);
        let mut m = ScrollModel::default();
        run_bounded(&mut m, DOWN, 5.0, level);
        assert!(m.offset.y > 720.0 - 1.0, "y should reach the level's far edge, got {}", m.offset.y);
    }

    /// Hold `dirs` for `secs` against a specific bound.
    fn run_bounded(m: &mut ScrollModel, dirs: HeldDirs, secs: f32, max: Vec2) {
        for _ in 0..((secs / DT).round() as usize) {
            m.step(dirs, DT, max);
        }
    }

    // ── The reachable range ─────────────────────────────────────────────────────

    #[test]
    fn the_offset_never_passes_the_supplied_maximum() {
        let level = Vec2::new(4000.0, 3000.0);
        let mut m = ScrollModel::default();
        for _ in 0..((30.0 / DT) as usize) {
            m.step(HeldDirs { right: true, down: true, ..NONE }, DT, level);
            assert!(
                m.offset.x <= level.x + 1e-3 && m.offset.y <= level.y + 1e-3,
                "offset {:?} passed the bound {level:?}",
                m.offset
            );
            assert!(m.offset.x >= 0.0 && m.offset.y >= 0.0, "and must stay non-negative");
        }
        assert!((m.offset.x - level.x).abs() < 1.0, "should rest at the bound, got {}", m.offset.x);
        assert!((m.offset.y - level.y).abs() < 1.0);
    }

    #[test]
    fn momentum_stops_dead_at_the_far_bound() {
        // Coasting at full speed a short way from the bound, the mirror of
        // `momentum_toward_origin_stops_exactly_at_zero`.
        let level = Vec2::new(1000.0, 800.0);
        let mut m = ScrollModel {
            offset: level - Vec2::new(20.0, 12.0),
            velocity: Vec2::new(MAX_SPEED, MAX_SPEED),
        };
        for _ in 0..600 {
            m.step(NONE, DT, level);
            assert!(m.offset.x <= level.x + 1e-3 && m.offset.y <= level.y + 1e-3, "overshot");
        }
        assert_eq!(m.offset, level, "should be pinned exactly at the bound");
        assert_eq!(m.velocity, Vec2::ZERO, "with no momentum left, and no bounce");
    }

    #[test]
    fn the_far_bound_leaves_exactly_one_viewport_of_slack() {
        // The distinction that matters: the view is clamped to the level *plus a screen*,
        // not to the level. A strict clamp would stop with the level's far edge at the
        // viewport's far edge (offset = level - viewport); this one goes exactly one
        // viewport further, so a whole empty screen past the boundary is reachable and
        // usable for placement. Checked across window sizes, including one larger than
        // the level, since the slack is expressed without a viewport term.
        let level = Vec2::new(4000.0, 3000.0);
        for viewport in [
            Vec2::new(1280.0, 700.0),
            Vec2::new(640.0, 360.0),
            Vec2::new(3000.0, 2400.0),
            Vec2::new(5000.0, 4000.0),
        ] {
            let mut m = ScrollModel::default();
            run_bounded(&mut m, HeldDirs { right: true, down: true, ..NONE }, 30.0, level);

            let strict = level - viewport;
            assert!(
                (m.offset.x - strict.x - viewport.x).abs() < 1.0,
                "viewport {viewport:?}: reachable offset {} should be one viewport past a \
                 strict clamp at {}",
                m.offset.x,
                strict.x
            );
            assert!((m.offset.y - strict.y - viewport.y).abs() < 1.0, "viewport {viewport:?}");

            // Which is to say: the level's far edge sits at the near edge of the view and
            // the rest of the screen is empty space beyond it.
            let visible_far = m.offset + viewport;
            assert!((m.offset.x - level.x).abs() < 1.0, "near edge is the level's far edge");
            assert!(((visible_far.x - level.x) - viewport.x).abs() < 1.0);
            assert!(((visible_far.y - level.y) - viewport.y).abs() < 1.0);
        }
    }

    #[test]
    fn the_bound_tracks_the_level_size_in_both_directions() {
        let small = Vec2::new(500.0, 400.0);
        let mut m = ScrollModel::default();
        run_bounded(&mut m, HeldDirs { right: true, down: true, ..NONE }, 10.0, small);
        assert!((m.offset.x - small.x).abs() < 1.0, "pinned to the small level");

        // Enlarging the level immediately allows going further.
        let big = Vec2::new(9000.0, 7000.0);
        run_bounded(&mut m, HeldDirs { right: true, down: true, ..NONE }, 5.0, big);
        assert!(m.offset.x > small.x + 100.0, "should scroll past the old bound, got {}", m.offset.x);

        // Shrinking it pulls an out-of-range offset straight back.
        m.step(NONE, DT, small);
        assert_eq!(m.offset, small, "an offset beyond a shrunk bound is pulled to it");
        assert_eq!(m.velocity, Vec2::ZERO);
    }

    #[test]
    fn a_degenerate_or_tiny_bound_never_inverts_the_range() {
        for max in [Vec2::ZERO, Vec2::new(1.0, 1.0), Vec2::new(-50.0, -50.0), Vec2::new(f32::NAN, f32::NAN)] {
            let mut m = ScrollModel::default();
            run_bounded(&mut m, HeldDirs { right: true, down: true, ..NONE }, 2.0, max);
            let expect = Vec2::new(
                if max.x.is_finite() { max.x.max(0.0) } else { 0.0 },
                if max.y.is_finite() { max.y.max(0.0) } else { 0.0 },
            );
            assert_eq!(m.offset, expect, "bound {max:?} should clamp to {expect:?}");
            assert!(m.offset.x >= 0.0 && m.offset.y >= 0.0, "and never go negative");
        }
    }

    #[test]
    fn never_negative_when_holding_toward_origin_at_rest() {
        let m = hold(LEFT, 2.0);
        assert_eq!(m.offset, Vec2::ZERO);
        let m = hold(UP, 2.0);
        assert_eq!(m.offset, Vec2::ZERO);
    }

    #[test]
    fn momentum_toward_origin_stops_exactly_at_zero() {
        let mut m = ScrollModel {
            offset: Vec2::new(10.0, 6.0),
            velocity: Vec2::new(-MAX_SPEED, -MAX_SPEED),
        };
        for _ in 0..600 {
            m.step(NONE, DT, FAR);
            assert!(m.offset.x >= 0.0 && m.offset.y >= 0.0, "offset went negative: {:?}", m.offset);
        }
        assert_eq!(m.offset, Vec2::ZERO, "must stop exactly at the origin");
        assert_eq!(m.velocity, Vec2::ZERO, "velocity must be zeroed at the origin");
    }

    #[test]
    fn reaches_max_speed_in_about_a_quarter_second() {
        let m = hold(RIGHT, 0.25);
        assert!(
            (m.velocity.x - MAX_SPEED).abs() < 1.0,
            "expected ~{} px/s after 0.25 s, got {}",
            MAX_SPEED,
            m.velocity.x
        );
        assert!(MAX_SPEED >= 600.0, "must not be slower than the previous fixed speed");
    }

    #[test]
    fn release_keeps_moving_then_stops_within_1_5_seconds() {
        let mut m = hold(RIGHT, 1.0);
        let x_at_release = m.offset.x;
        let v_at_release = m.velocity.x;
        assert!(v_at_release > 0.0);

        let mut elapsed = 0.0;
        let mut prev_speed = v_at_release;
        let mut moved_after_release = false;
        while m.is_moving() {
            m.step(NONE, DT, FAR);
            elapsed += DT;
            if m.offset.x > x_at_release {
                moved_after_release = true;
            }
            assert!(m.velocity.x.abs() <= prev_speed, "velocity must never re-accelerate on its own");
            prev_speed = m.velocity.x.abs();
            assert!(elapsed <= 1.5 + DT, "momentum must stop within 1.5 s, still moving at {elapsed}");
        }
        assert!(moved_after_release, "there must be momentum after release");
        assert_eq!(m.velocity, Vec2::ZERO, "must come to an exact stop");
        assert!(elapsed <= 1.5, "stopped at {elapsed}s");
    }

    #[test]
    fn no_perpetual_drift_after_stop() {
        let mut m = hold(RIGHT, 1.0);
        run(&mut m, NONE, 3.0);
        let settled = m.offset;
        run(&mut m, NONE, 3.0);
        assert_eq!(m.offset, settled, "offset must not creep once stopped");
        assert!(!m.is_moving());
    }

    #[test]
    fn opposite_keys_cancel_and_perpendicular_keys_combine() {
        let both = HeldDirs { left: true, right: true, ..NONE };
        let m = hold(both, 1.0);
        assert_eq!(m.offset, Vec2::ZERO, "Left+Right must cancel");

        let diagonal = HeldDirs { right: true, down: true, ..NONE };
        let m = hold(diagonal, 1.0);
        assert!(m.offset.x > 0.0 && m.offset.y > 0.0, "Right+Down must scroll diagonally");
    }

    #[test]
    fn pixel_offset_is_integer_valued_and_shared() {
        let m = hold(RIGHT, 0.1);
        let p = m.pixel_offset();
        assert_eq!(p.x, p.x.round());
        assert_eq!(p.y, p.y.round());
        assert_eq!(p, m.offset.round());
    }

    #[test]
    fn deterministic_for_identical_input_sequences() {
        let seq = [(RIGHT, 0.5), (DOWN, 0.3), (NONE, 0.4), (LEFT, 0.2), (NONE, 2.0)];
        let mut a = ScrollModel::default();
        let mut b = ScrollModel::default();
        for (dirs, secs) in seq {
            run(&mut a, dirs, secs);
            run(&mut b, dirs, secs);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn repaint_needed_only_while_held_or_moving() {
        let m = ScrollModel::default();
        assert!(!m.needs_repaint(NONE), "at rest: no repaint");
        assert!(m.needs_repaint(RIGHT), "key held: repaint");

        let mut m = hold(RIGHT, 0.5);
        assert!(m.needs_repaint(NONE), "coasting after release: repaint");
        run(&mut m, NONE, 3.0);
        assert!(!m.needs_repaint(NONE), "settled: no repaint");
    }

    #[test]
    fn ignores_invalid_dt() {
        let mut m = hold(RIGHT, 0.5);
        let before = m;
        m.step(RIGHT, f32::NAN, FAR);
        m.step(RIGHT, -1.0, FAR);
        assert_eq!(m, before);
    }
}
