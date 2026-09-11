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
    pub fn step(&mut self, held: HeldDirs, dt: f32) {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        let dir = held.axis_input();
        Self::step_axis(&mut self.offset.x, &mut self.velocity.x, dir.x, dt);
        Self::step_axis(&mut self.offset.y, &mut self.velocity.y, dir.y, dt);
    }

    fn step_axis(pos: &mut f32, vel: &mut f32, dir: f32, dt: f32) {
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

        // Clamp at the origin; momentum stops there instead of overshooting.
        if *pos < 0.0 {
            *pos = 0.0;
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

    fn hold(dirs: HeldDirs, secs: f32) -> ScrollModel {
        let mut m = ScrollModel::default();
        run(&mut m, dirs, secs);
        m
    }

    fn run(m: &mut ScrollModel, dirs: HeldDirs, secs: f32) {
        let steps = (secs / DT).round() as usize;
        for _ in 0..steps {
            m.step(dirs, DT);
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
    fn no_upper_bound_scrolls_past_background_and_content() {
        // The bundled level_image.png is 25600 x 720.
        let m = hold(RIGHT, 30.0);
        assert!(m.offset.x > 25600.0, "x must exceed the 25600 px background, got {}", m.offset.x);
        let m = hold(DOWN, 5.0);
        assert!(m.offset.y > 720.0, "y must exceed the 720 px background, got {}", m.offset.y);
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
            m.step(NONE, DT);
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
            m.step(NONE, DT);
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
        m.step(RIGHT, f32::NAN);
        m.step(RIGHT, -1.0);
        assert_eq!(m, before);
    }
}
