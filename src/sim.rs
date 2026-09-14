//! Pure kinematic character simulation for the editor's play mode.
//!
//! No egui `Context`/`Ui` and no editor state: the world is a list of convex polygons,
//! input is a plain struct, and [`Simulation::step`] advances by an explicit `dt`. That
//! keeps every trajectory in this module unit-testable headlessly.
//!
//! The behaviour mirrors `/Users/arikha/bevy_prince_platformer`, whose character is a
//! `bevy-tnua` floating controller on Rapier. Tnua cannot run here, so this is an
//! independent implementation of the same tuning ([`crate::game_config`]): close in
//! feel, not bit-identical.
//!
//! Coordinates are the editor's world space: top-left origin, Y increasing **downward**.
//! Gravity is +Y and jumping is -Y.

use egui::{Pos2, Vec2};

use crate::game_config as cfg;

/// How far below the capsule the ground sensor reaches. The game uses a downward box
/// cast with a hover gap; resting exactly on a surface produces no penetration, so
/// without a probe a dash (which suspends gravity) would read as airborne.
pub const GROUND_PROBE: f32 = 2.0;

/// Fixed simulation timestep. The accumulator in [`Simulation::advance`] keeps
/// trajectories identical regardless of display refresh rate.
pub const FIXED_DT: f32 = 1.0 / 120.0;
/// Guards against a huge `dt` after a stall turning into hundreds of catch-up steps.
pub const MAX_STEPS_PER_FRAME: u32 = 8;

/// Buttons held this frame, already mapped from keys by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    pub dash: bool,
}

impl Input {
    /// Net horizontal direction in {-1, 0, +1}; opposite keys cancel.
    fn axis_x(&self) -> f32 {
        (self.right as i8 - self.left as i8) as f32
    }
    /// Net vertical direction in {-1, 0, +1}, +1 = down (editor Y is down).
    fn axis_y(&self) -> f32 {
        (self.down as i8 - self.up as i8) as f32
    }
}

/// A convex collision polygon built from a level polygon entity.
///
/// The game builds each one as a `Collider::convex_hull`, so a concave polygon drawn in
/// the editor collides as its hull here too.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionPoly {
    pub points: Vec<Pos2>,
    /// `wall_tool` polygons are climbable; everything else is plain ground.
    pub climbable: bool,
}

impl CollisionPoly {
    /// Convex hull of `points` (monotone chain), matching the game's collider build.
    pub fn new(points: &[Pos2], climbable: bool) -> Option<Self> {
        let hull = convex_hull(points);
        (hull.len() >= 3).then_some(Self { points: hull, climbable })
    }
}

/// Andrew's monotone chain convex hull.
fn convex_hull(points: &[Pos2]) -> Vec<Pos2> {
    let mut pts: Vec<Pos2> = points.to_vec();
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Pos2, a: Pos2, b: Pos2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);

    let mut lower: Vec<Pos2> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Pos2> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// A rope hung from a level `rope_tool` entry: a pendulum the character can grab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rope {
    /// Anchor in world space (the rope hangs toward +Y).
    pub anchor: Pos2,
    pub length: f32,
}

/// Everything solid in the level, plus its ropes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    pub polys: Vec<CollisionPoly>,
    pub ropes: Vec<Rope>,
}

/// A rope's swing: angle from vertical (0 = hanging straight down, positive toward +x)
/// and angular velocity. Lives in the simulation because play never edits the level.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RopeSwing {
    pub angle: f32,
    pub ang_vel: f32,
}

impl RopeSwing {
    /// Unit vector along the rope from its anchor.
    pub fn dir(&self) -> Vec2 {
        Vec2::new(self.angle.sin(), self.angle.cos())
    }
    /// Unit tangent in the direction of increasing angle.
    pub fn tangent(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), -self.angle.sin())
    }
}

/// Below these the free rope snaps to hanging still.
const ROPE_REST_ANGLE: f32 = 0.5_f32 * std::f32::consts::PI / 180.0;
const ROPE_REST_ANG_VEL: f32 = 0.02;

/// What the character is doing, for the status overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Grounded,
    Airborne,
    Dashing,
    WallSliding,
    Climbing,
    Rope,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Grounded => "run",
            State::Airborne => "air",
            State::Dashing => "dash",
            State::WallSliding => "wall slide",
            State::Climbing => "climb",
            State::Rope => "rope",
        }
    }
}

/// The simulated character.
#[derive(Debug, Clone)]
pub struct Simulation {
    /// Capsule centre in world space.
    pub pos: Pos2,
    pub vel: Vec2,
    pub state: State,
    pub grounded: bool,
    /// -1 or +1; which way the character last moved.
    pub facing: f32,

    spawn: Pos2,
    accumulator: f32,

    // Jump
    coyote_left: f32,
    jump_buffer_left: f32,
    jump_was_held: bool,
    /// True from takeoff until the next landing: selects the jump gravity curve.
    in_jump: bool,
    jump_held_since_takeoff: bool,

    // Dash
    dash_left: f32,
    dash_cooldown_left: f32,
    dash_dir: f32,
    dash_was_held: bool,
    dash_buffer_left: f32,
    air_dashes_used: usize,

    // Wall
    wall_dir: f32,
    wall_coyote_left: f32,
    wall_coyote_dir: f32,
    wall_climbable: bool,
    input_lock_left: f32,
    /// Horizontal velocity is held (not decelerated) for the arc of a wall jump when the
    /// player gives no horizontal input, so the jump carries its nominal distance.
    wall_jump_carry: bool,
    /// While positive, every button is ignored: the hurt lockout after a club hit.
    /// Gravity and collision keep running, so the character still falls and lands.
    control_lock_left: f32,
    /// This rise is a bounce (a pogo or a knockback pop), not a held jump, so the
    /// release-shorten gravity must never apply to it — otherwise it is cancelled on
    /// the very next step. The faster post-apex fall still applies, as in the game.
    bounce: bool,

    // Rope
    /// One swing per `World::ropes` entry, in the same order; resized on each step.
    rope_swings: Vec<RopeSwing>,
    /// The rope being held, as `(index, hold length from the anchor)`.
    rope_hold: Option<(usize, f32)>,
    /// No grab while positive: the lock after letting go.
    regrab_lock_left: f32,
}

impl Simulation {
    pub fn new(spawn: Pos2) -> Self {
        Self {
            pos: spawn,
            vel: Vec2::ZERO,
            state: State::Airborne,
            grounded: false,
            facing: 1.0,
            spawn,
            accumulator: 0.0,
            coyote_left: 0.0,
            jump_buffer_left: 0.0,
            jump_was_held: false,
            in_jump: false,
            jump_held_since_takeoff: false,
            dash_left: 0.0,
            dash_cooldown_left: 0.0,
            dash_dir: 1.0,
            dash_was_held: false,
            dash_buffer_left: 0.0,
            air_dashes_used: 0,
            wall_dir: 0.0,
            wall_coyote_left: 0.0,
            wall_coyote_dir: 0.0,
            wall_climbable: false,
            input_lock_left: 0.0,
            wall_jump_carry: false,
            control_lock_left: 0.0,
            bounce: false,
            rope_swings: Vec::new(),
            rope_hold: None,
            regrab_lock_left: 0.0,
        }
    }

    /// Move the spawn point and restart there.
    pub fn respawn_at(&mut self, spawn: Pos2) {
        let acc = self.accumulator;
        *self = Self::new(spawn);
        self.accumulator = acc;
    }

    pub fn respawn(&mut self) {
        let spawn = self.spawn;
        self.respawn_at(spawn);
    }

    /// Launch velocity of a full-hold jump: `v = sqrt(2 g h)` so the apex is exactly
    /// [`cfg::JUMP_HEIGHT`] under the rising gravity.
    fn jump_speed(height: f32) -> f32 {
        (2.0 * cfg::GRAVITY * height).sqrt()
    }

    /// Ignore every button for `secs`, as the game's hurt lockout does. Gravity and
    /// collision continue, so the character still falls, lands and is carried by any
    /// knockback already applied.
    pub fn lock_control(&mut self, secs: f32) {
        self.control_lock_left = self.control_lock_left.max(secs);
        // Being hurt drops the rope.
        self.drop_rope();
    }

    pub fn is_control_locked(&self) -> bool {
        self.control_lock_left > 0.0
    }

    /// True while a knockback or wall jump is holding the velocity against input.
    pub fn is_input_locked(&self) -> bool {
        self.input_lock_left > 0.0
    }

    /// Set the velocity outright and hold it against horizontal input for `hold` seconds:
    /// sword recoil and club knockback.
    pub fn apply_knockback(&mut self, vel: Vec2, hold: f32) {
        self.drop_rope();
        self.vel = vel;
        self.input_lock_left = self.input_lock_left.max(hold);
        self.wall_jump_carry = true;
        self.dash_left = 0.0;
        if vel.y < 0.0 {
            self.in_jump = true;
            self.bounce = true;
        }
    }

    /// Bounce off a down-slash hit: rise `height` px and get the air dash back, as the
    /// game's pogo does.
    pub fn pogo(&mut self, height: f32) {
        self.vel.y = -Self::jump_speed(height);
        self.air_dashes_used = 0;
        // A pogo is a fixed bounce, not a held jump: without `bounce` the
        // release-shorten gravity would cancel the rise on the very next step. Keeping
        // `in_jump` set preserves the game's faster post-apex descent.
        self.in_jump = true;
        self.bounce = true;
    }

    pub fn is_dashing(&self) -> bool {
        self.dash_left > 0.0
    }

    pub fn is_climbing(&self) -> bool {
        self.state == State::Climbing
    }

    /// Part of the simulation's public contract (used by tests and the game port).
    #[allow(dead_code)]
    pub fn is_on_rope(&self) -> bool {
        self.rope_hold.is_some()
    }

    /// The rope being held and the hold length, if any.
    #[allow(dead_code)]
    pub fn rope_hold(&self) -> Option<(usize, f32)> {
        self.rope_hold
    }

    /// The current swing of rope `i` (hanging still if unknown).
    pub fn rope_swing(&self, i: usize) -> RopeSwing {
        self.rope_swings.get(i).copied().unwrap_or_default()
    }

    /// Let go without a hop: the character keeps its velocity, the rope keeps swinging.
    fn drop_rope(&mut self) {
        if self.rope_hold.take().is_some() {
            self.state = State::Airborne;
        }
    }

    /// Keep one swing per rope in the world. Ropes are level data and never change
    /// during play, so an existing swing is kept; a fresh world starts every rope still.
    fn sync_ropes(&mut self, world: &World) {
        if self.rope_swings.len() != world.ropes.len() {
            self.rope_swings = vec![RopeSwing::default(); world.ropes.len()];
            self.rope_hold = None;
        }
    }

    /// Advance every rope nobody is holding as a damped free pendulum.
    fn step_free_ropes(&mut self, world: &World, dt: f32) {
        let held = self.rope_hold.map(|(i, _)| i);
        for (i, (rope, swing)) in world.ropes.iter().zip(self.rope_swings.iter_mut()).enumerate() {
            if held == Some(i) {
                continue;
            }
            let len = rope.length.max(cfg::ROPE_MIN_HOLD);
            swing.ang_vel += (-(cfg::GRAVITY / len) * swing.angle.sin() - cfg::ROPE_DAMPING * swing.ang_vel) * dt;
            swing.angle += swing.ang_vel * dt;
            if swing.angle.abs() < ROPE_REST_ANGLE && swing.ang_vel.abs() < ROPE_REST_ANG_VEL {
                *swing = RopeSwing::default();
            }
        }
    }

    /// Grab the nearest rope within reach when up or down is held.
    fn try_grab_rope(&mut self, world: &World, input: Input) {
        if self.regrab_lock_left > 0.0 || input.axis_y() == 0.0 || self.is_dashing() {
            return;
        }
        let mut best: Option<(usize, f32, f32)> = None; // (index, hold length, distance)
        for (i, rope) in world.ropes.iter().enumerate() {
            let swing = self.rope_swings[i];
            let d = swing.dir();
            let start = rope.anchor + d * cfg::ROPE_MIN_HOLD.min(rope.length);
            let end = rope.anchor + d * rope.length;
            let nearest = closest_on_segment(start, end, self.pos);
            let dist = (self.pos - nearest).length();
            if dist <= cfg::ROPE_GRAB_HALF_WIDTH && best.is_none_or(|(_, _, bd)| dist < bd) {
                let along = (self.pos - rope.anchor).dot(d);
                let hold = along.clamp(cfg::ROPE_MIN_HOLD.min(rope.length), rope.length);
                best = Some((i, hold, dist));
            }
        }
        let Some((i, hold, _)) = best else { return };
        let swing = &mut self.rope_swings[i];
        // Momentum carries into the swing.
        swing.ang_vel = self.vel.dot(swing.tangent()) / hold;
        let d = swing.dir();
        let t = swing.tangent();
        self.pos = world.ropes[i].anchor + d * hold;
        self.vel = t * (hold * swing.ang_vel);
        self.rope_hold = Some((i, hold));
        self.state = State::Rope;
        self.grounded = false;
        self.in_jump = false;
        self.bounce = false;
        self.wall_dir = 0.0;
        self.wall_jump_carry = false;
        self.coyote_left = 0.0;
        self.jump_buffer_left = 0.0;
    }

    /// One step on the rope: release on jump, else swing, pump, climb and collide.
    fn step_rope(&mut self, world: &World, input: Input, dt: f32) {
        let Some((i, mut hold)) = self.rope_hold else { return };
        let rope = world.ropes[i];

        if self.jump_buffer_left > 0.0 {
            // Let go with the swing's velocity plus a hop; the rope swings on.
            let swing = self.rope_swings[i];
            self.vel = swing.tangent() * (hold * swing.ang_vel)
                + Vec2::new(0.0, -Self::jump_speed(cfg::ROPE_RELEASE_HOP_HEIGHT));
            self.rope_hold = None;
            self.state = State::Airborne;
            self.in_jump = true;
            self.bounce = true;
            self.jump_held_since_takeoff = true;
            self.air_dashes_used = 0;
            self.jump_buffer_left = 0.0;
            self.regrab_lock_left = cfg::ROPE_REGRAB_LOCK;
            return;
        }

        let max_angle = cfg::ROPE_MAX_ANGLE_DEG.to_radians();
        let swing = &mut self.rope_swings[i];
        let pump = input.axis_x() * cfg::ROPE_PUMP_ACCEL;
        swing.ang_vel += (-(cfg::GRAVITY / hold) * swing.angle.sin() + pump / hold
            - cfg::ROPE_DAMPING * swing.ang_vel)
            * dt;
        swing.angle += swing.ang_vel * dt;
        if swing.angle > max_angle {
            swing.angle = max_angle;
            swing.ang_vel = swing.ang_vel.min(0.0);
        } else if swing.angle < -max_angle {
            swing.angle = -max_angle;
            swing.ang_vel = swing.ang_vel.max(0.0);
        }
        // Down lengthens the hold, up shortens it.
        hold = (hold + input.axis_y() * cfg::CLIMB_SPEED * dt)
            .clamp(cfg::ROPE_MIN_HOLD.min(rope.length), rope.length);

        let swing = self.rope_swings[i];
        self.pos = rope.anchor + swing.dir() * hold;
        self.vel = swing.tangent() * (hold * swing.ang_vel);
        if swing.ang_vel.abs() > 1e-3 {
            self.facing = swing.ang_vel.signum();
        }

        // Solid geometry stops the swing: push out, re-aim the rope at the pushed
        // position, and kill the angular velocity.
        let mut pushed = false;
        for _ in 0..4 {
            let Some((mtv, _)) = self.deepest_overlap(world) else { break };
            self.pos += mtv;
            pushed = true;
        }
        if pushed {
            let rel = self.pos - rope.anchor;
            let swing = &mut self.rope_swings[i];
            swing.angle = rel.x.atan2(rel.y).clamp(-max_angle, max_angle);
            swing.ang_vel = 0.0;
            self.vel = Vec2::ZERO;
        }

        self.rope_hold = Some((i, hold));
        self.state = State::Rope;
        self.grounded = false;
        self.wall_dir = 0.0;
        self.wall_climbable = false;
    }

    /// Axis-aligned bounds of the capsule, for overlap tests against boxes.
    pub fn aabb(&self) -> egui::Rect {
        egui::Rect::from_center_size(
            self.pos,
            Vec2::new(
                cfg::PLAYER_CAPSULE_RADIUS * 2.0,
                cfg::PLAYER_CAPSULE_HALF_EXTENT_Y * 2.0,
            ),
        )
    }

    /// Advance real time, running whole fixed steps. Returns the number of steps taken.
    ///
    /// Movement only. The editor drives [`crate::combat::PlaySession::advance`] instead,
    /// which runs movement and combat on one accumulator; this entry point is kept as the
    /// movement-only contract the canvas-scrolling / play-simulation specs are stated in.
    #[allow(dead_code)]
    pub fn advance(&mut self, world: &World, input: Input, dt: f32) -> u32 {
        if !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.accumulator += dt;
        let mut steps = 0;
        while self.accumulator >= FIXED_DT && steps < MAX_STEPS_PER_FRAME {
            self.step(world, input, FIXED_DT);
            self.accumulator -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_STEPS_PER_FRAME {
            self.accumulator = 0.0; // Drop the backlog rather than spiral.
        }
        steps
    }

    /// One fixed step. Public so tests can drive an exact number of them.
    pub fn step(&mut self, world: &World, input: Input, dt: f32) {
        // The hurt lockout suppresses input without pausing physics.
        self.control_lock_left = (self.control_lock_left - dt).max(0.0);
        let input = if self.control_lock_left > 0.0 { Input::default() } else { input };

        self.tick_timers(dt, input);
        self.regrab_lock_left = (self.regrab_lock_left - dt).max(0.0);
        self.sync_ropes(world);
        self.step_free_ropes(world, dt);

        if self.rope_hold.is_some() {
            self.step_rope(world, input, dt);
            return;
        }

        if self.dash_left > 0.0 {
            self.step_dash(world, dt);
            return;
        }

        let climbing = self.wall_climbable
            && self.wall_dir != 0.0
            && input.axis_x() == self.wall_dir
            && !self.grounded;

        if climbing {
            self.step_climb(world, input, dt);
            return;
        }

        self.try_start_dash(input);
        if self.dash_left > 0.0 {
            self.step_dash(world, dt);
            return;
        }

        self.step_horizontal(input, dt);
        self.step_jump(input);
        self.step_gravity(input, dt);
        self.integrate(world, dt);

        self.state = if self.grounded {
            State::Grounded
        } else if self.wall_dir != 0.0 && self.vel.y > 0.0 {
            State::WallSliding
        } else {
            State::Airborne
        };

        self.try_grab_rope(world, input);
    }

    fn tick_timers(&mut self, dt: f32, input: Input) {
        let dec = |t: &mut f32| *t = (*t - dt).max(0.0);
        dec(&mut self.coyote_left);
        dec(&mut self.jump_buffer_left);
        dec(&mut self.dash_cooldown_left);
        dec(&mut self.wall_coyote_left);
        dec(&mut self.input_lock_left);
        dec(&mut self.dash_buffer_left);

        if input.jump && !self.jump_was_held {
            self.jump_buffer_left = cfg::JUMP_BUFFER_TIME;
        }
        if input.dash && !self.dash_was_held {
            self.dash_buffer_left = cfg::DASH_BUFFER_TIME;
        }
        self.jump_was_held = input.jump;
        self.dash_was_held = input.dash;
    }

    fn try_start_dash(&mut self, input: Input) {
        if self.dash_buffer_left <= 0.0 || self.dash_cooldown_left > 0.0 {
            return;
        }
        if !self.grounded && self.air_dashes_used >= cfg::MAX_AIR_DASHES {
            return;
        }
        let dir = if input.axis_x() != 0.0 { input.axis_x() } else { self.facing };
        self.dash_dir = dir;
        self.facing = dir;
        self.dash_left = cfg::DASH_DISTANCE / cfg::DASH_SPEED;
        self.dash_cooldown_left = cfg::DASH_COOLDOWN;
        self.dash_buffer_left = 0.0;
        if !self.grounded {
            self.air_dashes_used += 1;
        }
        self.state = State::Dashing;
    }

    fn step_dash(&mut self, world: &World, dt: f32) {
        self.vel = Vec2::new(self.dash_dir * cfg::DASH_SPEED, 0.0);
        self.dash_left = (self.dash_left - dt).max(0.0);
        self.integrate(world, dt);
        if self.dash_left <= 0.0 {
            // Hand back to the run basis at the braked speed.
            self.vel.x = self.dash_dir * cfg::DASH_BRAKE_TO_SPEED;
        }
        self.state = State::Dashing;
    }

    fn step_climb(&mut self, world: &World, input: Input, dt: f32) {
        self.vel = Vec2::new(
            self.wall_dir * cfg::CLIMB_WALL_DRIFT,
            input.axis_y() * cfg::CLIMB_SPEED,
        );
        self.integrate(world, dt);
        self.state = State::Climbing;
    }

    fn step_horizontal(&mut self, input: Input, dt: f32) {
        let dir = input.axis_x();
        if self.input_lock_left > 0.0 {
            return; // Wall jump keeps its momentum.
        }
        if dir == 0.0 && self.wall_jump_carry && !self.grounded {
            return; // No input during a wall-jump arc: hold the launch velocity.
        }
        if dir != 0.0 {
            self.facing = dir;
            self.wall_jump_carry = false;
        }
        let accel = if self.grounded { cfg::PLAYER_GROUND_ACCEL } else { cfg::PLAYER_AIR_ACCEL };
        let target = dir * cfg::PLAYER_RUN_SPEED;
        let dv = accel * dt;
        self.vel.x = if self.vel.x < target {
            (self.vel.x + dv).min(target)
        } else {
            (self.vel.x - dv).max(target)
        };
    }

    fn step_jump(&mut self, input: Input) {
        if self.jump_buffer_left <= 0.0 {
            return;
        }
        // Wall jump takes priority while on (or just off) a wall in the air.
        let wall_dir = if self.wall_dir != 0.0 {
            self.wall_dir
        } else if self.wall_coyote_left > 0.0 {
            self.wall_coyote_dir
        } else {
            0.0
        };
        if !self.grounded && self.coyote_left <= 0.0 && wall_dir != 0.0 {
            let vy = Self::jump_speed(cfg::WALL_JUMP_HEIGHT);
            let t_rise = vy / cfg::GRAVITY;
            let g_fall = cfg::GRAVITY + cfg::JUMP_FALL_EXTRA_GRAVITY;
            let t_fall = (2.0 * cfg::WALL_JUMP_HEIGHT / g_fall).sqrt();
            self.vel = Vec2::new(
                -wall_dir * (cfg::WALL_JUMP_HORIZONTAL_DISTANCE / (t_rise + t_fall)),
                -vy,
            );
            self.facing = -wall_dir;
            self.bounce = false;
            self.input_lock_left = cfg::WALL_JUMP_INPUT_LOCK;
            self.wall_jump_carry = true;
            self.jump_buffer_left = 0.0;
            self.wall_coyote_left = 0.0;
            self.wall_dir = 0.0;
            self.in_jump = true;
            self.jump_held_since_takeoff = true;
            return;
        }
        if self.grounded || self.coyote_left > 0.0 {
            self.vel.y = -Self::jump_speed(cfg::JUMP_HEIGHT);
            self.bounce = false;
            self.jump_buffer_left = 0.0;
            self.coyote_left = 0.0;
            self.grounded = false;
            self.in_jump = true;
            self.jump_held_since_takeoff = true;
            let _ = input;
        }
    }

    fn step_gravity(&mut self, input: Input, dt: f32) {
        if self.in_jump && !input.jump {
            self.jump_held_since_takeoff = false;
        }
        let rising = self.vel.y < 0.0;
        let mut g = cfg::GRAVITY;
        if self.in_jump {
            if rising {
                if !self.jump_held_since_takeoff && !self.bounce {
                    g += cfg::JUMP_SHORTEN_EXTRA_GRAVITY;
                }
            } else {
                g += cfg::JUMP_FALL_EXTRA_GRAVITY;
            }
        } else if !self.grounded && !rising {
            g += cfg::PLAYER_FREE_FALL_EXTRA_GRAVITY;
        }
        if input.down && !rising {
            g *= cfg::FAST_FALL_GRAVITY_MULT;
        }
        self.vel.y += g * dt;

        let cap = if self.wall_dir != 0.0 && self.vel.y > 0.0 {
            cfg::WALL_SLIDE_MAX_FALL_SPEED
        } else {
            cfg::MAX_FALL_SPEED
        };
        if self.vel.y > cap {
            self.vel.y = cap;
        }
    }

    /// Move by the current velocity in sub-steps small enough that the capsule can never
    /// pass through a polygon, then resolve any overlap.
    fn integrate(&mut self, world: &World, dt: f32) {
        let was_grounded = self.grounded;
        let travel = (self.vel * dt).length();
        // Never advance more than a quarter of the capsule radius per sub-step.
        let sub = ((travel / (cfg::PLAYER_CAPSULE_RADIUS * 0.25)).ceil() as u32).clamp(1, 16);
        let sdt = dt / sub as f32;

        let mut grounded;
        grounded = false;
        let mut wall_dir = 0.0;
        let mut wall_climbable = false;

        for _ in 0..sub {
            self.pos += self.vel * sdt;
            for _ in 0..4 {
                let Some((mtv, climbable)) = self.deepest_overlap(world) else { break };
                self.pos += mtv;
                let n = mtv.normalized();
                if n.y < -0.5 {
                    grounded = true;
                    if self.vel.y > 0.0 {
                        self.vel.y = 0.0;
                    }
                } else if n.y > 0.5 {
                    if self.vel.y < 0.0 {
                        self.vel.y = 0.0;
                    }
                } else {
                    wall_dir = -n.x.signum();
                    wall_climbable = climbable;
                    if self.vel.x.signum() == -n.x.signum() {
                        self.vel.x = 0.0;
                    }
                }
            }
        }

        // A capsule resting exactly on a surface does not penetrate it, so confirm
        // ground with a short downward probe as well.
        if !grounded && self.vel.y >= 0.0 {
            let probe = Pos2::new(self.pos.x, self.pos.y + GROUND_PROBE);
            grounded = world.polys.iter().any(|poly| {
                capsule_poly_mtv(
                    probe,
                    cfg::PLAYER_CAPSULE_HALF_HEIGHT,
                    cfg::PLAYER_CAPSULE_RADIUS,
                    &poly.points,
                )
                .is_some_and(|m| m.normalized().y < -0.5)
            });
        }

        self.grounded = grounded;
        if wall_dir != 0.0 {
            self.wall_dir = wall_dir;
            self.wall_climbable = wall_climbable;
            self.wall_coyote_left = cfg::WALL_COYOTE_TIME;
            self.wall_coyote_dir = wall_dir;
        } else {
            self.wall_dir = 0.0;
            self.wall_climbable = false;
        }

        if grounded {
            self.coyote_left = cfg::COYOTE_TIME;
            self.air_dashes_used = 0;
            self.in_jump = false;
            self.bounce = false;
            self.wall_jump_carry = false;
        } else if was_grounded {
            self.coyote_left = cfg::COYOTE_TIME; // Just walked off a ledge.
        }
    }

    /// Largest penetration of the capsule into any polygon, as the vector that resolves
    /// it, plus whether that polygon is climbable.
    fn deepest_overlap(&self, world: &World) -> Option<(Vec2, bool)> {
        let mut best: Option<(Vec2, bool, f32)> = None;
        for poly in &world.polys {
            if let Some(mtv) = capsule_poly_mtv(
                self.pos,
                cfg::PLAYER_CAPSULE_HALF_HEIGHT,
                cfg::PLAYER_CAPSULE_RADIUS,
                &poly.points,
            ) {
                let d = mtv.length();
                if d > best.as_ref().map_or(1e-4, |(_, _, bd)| *bd) {
                    best = Some((mtv, poly.climbable, d));
                }
            }
        }
        best.map(|(m, c, _)| (m, c))
    }
}

/// Closest point to `p` on segment `a`-`b`.
fn closest_on_segment(a: Pos2, b: Pos2, p: Pos2) -> Pos2 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return a;
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    a + ab * t
}

/// Minimum translation vector pushing a vertical capsule out of a convex polygon, or
/// `None` when they do not overlap.
///
/// SAT over the polygon's edge normals plus the axes from each polygon vertex to the
/// closest point on the capsule's segment, which is what accounts for the round caps.
pub fn capsule_poly_mtv(
    center: Pos2,
    half_height: f32,
    radius: f32,
    poly: &[Pos2],
) -> Option<Vec2> {
    if poly.len() < 3 {
        return None;
    }
    let top = Pos2::new(center.x, center.y - half_height);
    let bottom = Pos2::new(center.x, center.y + half_height);

    let mut axes: Vec<Vec2> = Vec::with_capacity(poly.len() * 2 + 1);
    for i in 0..poly.len() {
        let e = poly[(i + 1) % poly.len()] - poly[i];
        if e.length_sq() > f32::EPSILON {
            axes.push(Vec2::new(-e.y, e.x).normalized());
        }
    }
    axes.push(Vec2::X);
    for &v in poly {
        let c = closest_on_segment(top, bottom, v);
        let d = v - c;
        if d.length_sq() > f32::EPSILON {
            axes.push(d.normalized());
        }
    }

    let mut best_axis = Vec2::ZERO;
    let mut best_overlap = f32::INFINITY;
    for axis in axes {
        let (mut pmin, mut pmax) = (f32::INFINITY, f32::NEG_INFINITY);
        for &v in poly {
            let d = axis.dot(v.to_vec2());
            pmin = pmin.min(d);
            pmax = pmax.max(d);
        }
        let (t, b) = (axis.dot(top.to_vec2()), axis.dot(bottom.to_vec2()));
        let cmin = t.min(b) - radius;
        let cmax = t.max(b) + radius;

        let overlap = pmax.min(cmax) - pmin.max(cmin);
        if overlap <= 0.0 {
            return None; // Separating axis found.
        }
        if overlap < best_overlap {
            best_overlap = overlap;
            // Point the axis from the polygon toward the capsule.
            let sign = if (t + b) * 0.5 < (pmin + pmax) * 0.5 { -1.0 } else { 1.0 };
            best_axis = axis * sign;
        }
    }
    (best_overlap.is_finite() && best_overlap > 0.0).then(|| best_axis * best_overlap)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = FIXED_DT;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32, climbable: bool) -> CollisionPoly {
        CollisionPoly::new(
            &[
                Pos2::new(x0, y0),
                Pos2::new(x1, y0),
                Pos2::new(x1, y1),
                Pos2::new(x0, y1),
            ],
            climbable,
        )
        .expect("rect is a valid hull")
    }

    /// A wide floor with its surface at y = 500.
    fn floor_world() -> World {
        World { polys: vec![rect(-5000.0, 500.0, 5000.0, 900.0, false)], ropes: vec![] }
    }

    fn none() -> Input {
        Input::default()
    }

    fn run_for(sim: &mut Simulation, world: &World, input: Input, secs: f32) {
        for _ in 0..((secs / DT).round() as usize) {
            sim.step(world, input, DT);
        }
    }

    /// Drop onto the floor and settle.
    fn standing() -> (Simulation, World) {
        let w = floor_world();
        let mut s = Simulation::new(Pos2::new(0.0, 400.0));
        run_for(&mut s, &w, none(), 1.0);
        assert!(s.grounded, "should have landed, pos {:?}", s.pos);
        (s, w)
    }

    // ── Ropes ────────────────────────────────────────────────────────────────

    const ROPE_ANCHOR: Pos2 = Pos2::new(500.0, 100.0);
    const ROPE_LEN: f32 = 300.0;

    fn rope_world() -> World {
        World { polys: vec![], ropes: vec![Rope { anchor: ROPE_ANCHOR, length: ROPE_LEN }] }
    }

    const UP: Input = Input { up: true, ..NONE_INPUT };
    const NONE_INPUT: Input = Input { left: false, right: false, up: false, down: false, jump: false, dash: false };

    /// A character hanging still on the rope at `hold` px below the anchor.
    fn hanging(hold: f32) -> (Simulation, World) {
        let world = rope_world();
        let mut sim = Simulation::new(Pos2::new(ROPE_ANCHOR.x, ROPE_ANCHOR.y + hold));
        sim.vel = Vec2::ZERO;
        sim.step(&world, UP, DT);
        assert!(sim.is_on_rope(), "should have grabbed the rope");
        (sim, world)
    }

    #[test]
    fn holding_up_beside_a_rope_grabs_it_at_that_height() {
        let world = rope_world();
        let mut sim = Simulation::new(Pos2::new(ROPE_ANCHOR.x + 20.0, ROPE_ANCHOR.y + 200.0));
        sim.step(&world, none(), DT);
        assert!(!sim.is_on_rope(), "no grab without up or down held");
        sim.step(&world, UP, DT);
        assert!(sim.is_on_rope());
        assert_eq!(sim.state, State::Rope);
        assert_eq!(sim.state.label(), "rope");
        let (_, hold) = sim.rope_hold().unwrap();
        assert!((hold - 200.0).abs() < 5.0, "hold at the grab height, got {hold}");
        assert!((sim.pos.x - ROPE_ANCHOR.x).abs() < 1e-3, "snapped onto the rope line");
    }

    #[test]
    fn a_rope_out_of_reach_or_above_the_min_hold_is_not_grabbed() {
        let world = rope_world();
        let mut sim = Simulation::new(Pos2::new(ROPE_ANCHOR.x + 60.0, ROPE_ANCHOR.y + 200.0));
        sim.step(&world, UP, DT);
        assert!(!sim.is_on_rope(), "60 px away is beyond the 30 px reach");
        let mut sim = Simulation::new(Pos2::new(ROPE_ANCHOR.x, ROPE_ANCHOR.y + 5.0));
        sim.step(&world, UP, DT);
        assert!(!sim.is_on_rope(), "right at the anchor is above the lowest hold");
    }

    #[test]
    fn a_rope_swings_with_the_pendulum_period_and_settles() {
        let (mut sim, world) = hanging(200.0);
        let hold = 200.0;
        // Start at 30 degrees with no motion.
        sim.rope_swings[0] = RopeSwing { angle: 30_f32.to_radians(), ang_vel: 0.0 };
        // Time until the angle first comes back through 30°-ish from the other side:
        // measure the period as twice the time between successive zero crossings.
        let mut crossings = vec![];
        let mut prev = sim.rope_swing(0).angle;
        let mut t = 0.0;
        while crossings.len() < 3 && t < 10.0 {
            sim.step(&world, none(), DT);
            t += DT;
            let a = sim.rope_swing(0).angle;
            if (prev > 0.0) != (a > 0.0) {
                crossings.push(t);
            }
            prev = a;
        }
        assert_eq!(crossings.len(), 3, "should swing through the bottom repeatedly");
        let period = crossings[2] - crossings[0];
        let expected = 2.0 * std::f32::consts::PI * (hold / cfg::GRAVITY).sqrt();
        assert!((period - expected).abs() < expected * 0.10, "period {period} vs {expected}");
        assert!(sim.is_on_rope(), "still holding on");

        run_for(&mut sim, &world, none(), 30.0);
        assert!(sim.rope_swing(0).angle.abs() < 0.01, "damped to hanging straight, got {}", sim.rope_swing(0).angle);
    }

    #[test]
    fn pumping_builds_a_swing_capped_at_the_max_angle() {
        let (mut sim, world) = hanging(200.0);
        let right = Input { right: true, ..NONE_INPUT };
        let mut max_seen = 0.0_f32;
        let max_angle = cfg::ROPE_MAX_ANGLE_DEG.to_radians();
        for _ in 0..(20.0 / DT) as usize {
            sim.step(&world, right, DT);
            let a = sim.rope_swing(0).angle;
            assert!(a <= max_angle + 1e-4, "angle {a} passed the cap");
            max_seen = max_seen.max(a);
        }
        assert!(max_seen > 20_f32.to_radians(), "pumping should build a real swing, got {max_seen}");
        assert!((max_seen - max_angle).abs() < 0.05, "and reach the cap, got {max_seen}");
    }

    #[test]
    fn up_and_down_move_the_hold_along_the_rope_within_bounds() {
        let (mut sim, world) = hanging(200.0);
        run_for(&mut sim, &world, UP, 0.5);
        let (_, hold) = sim.rope_hold().unwrap();
        assert!((hold - (200.0 - cfg::CLIMB_SPEED * 0.5)).abs() < 2.0, "climbed up, got {hold}");
        run_for(&mut sim, &world, UP, 5.0);
        assert!((sim.rope_hold().unwrap().1 - cfg::ROPE_MIN_HOLD).abs() < 1e-3, "no closer than the min hold");
        let down = Input { down: true, ..NONE_INPUT };
        run_for(&mut sim, &world, down, 5.0);
        assert!((sim.rope_hold().unwrap().1 - ROPE_LEN).abs() < 1e-3, "no further than the rope's end");
        assert!((sim.pos.y - (ROPE_ANCHOR.y + ROPE_LEN)).abs() < 1e-2, "hanging at the very end");
    }

    #[test]
    fn jump_releases_with_the_swing_velocity_plus_a_hop_and_locks_regrab() {
        let (mut sim, world) = hanging(200.0);
        sim.rope_swings[0] = RopeSwing { angle: 0.0, ang_vel: 2.0 };
        sim.step(&world, none(), DT);
        let expect_vx = 200.0 * sim.rope_swing(0).ang_vel;
        let jump = Input { jump: true, ..NONE_INPUT };
        sim.step(&world, jump, DT);
        assert!(!sim.is_on_rope(), "let go");
        assert_eq!(sim.state, State::Airborne);
        assert!((sim.vel.x - expect_vx).abs() < 5.0, "keeps the tangential speed, got {} vs {expect_vx}", sim.vel.x);
        let hop = (2.0 * cfg::GRAVITY * cfg::ROPE_RELEASE_HOP_HEIGHT).sqrt();
        assert!(sim.vel.y < -hop + 20.0, "plus the hop, got {}", sim.vel.y);
        assert!(sim.rope_swing(0).ang_vel.abs() > 1.0, "the rope swings on");

        // Held up right at the rope: no grab during the lock, grab after it.
        sim.pos = Pos2::new(ROPE_ANCHOR.x, ROPE_ANCHOR.y + 200.0);
        sim.vel = Vec2::ZERO;
        sim.step(&world, UP, DT);
        assert!(!sim.is_on_rope(), "regrab is locked");
        for _ in 0..(cfg::ROPE_REGRAB_LOCK / DT) as usize + 2 {
            sim.pos = Pos2::new(ROPE_ANCHOR.x, ROPE_ANCHOR.y + 200.0);
            sim.vel = Vec2::ZERO;
            sim.step(&world, none(), DT);
        }
        // The rope swung on meanwhile; still it so the character is on its line again.
        sim.rope_swings[0] = RopeSwing::default();
        sim.pos = Pos2::new(ROPE_ANCHOR.x, ROPE_ANCHOR.y + 200.0);
        sim.vel = Vec2::ZERO;
        sim.step(&world, UP, DT);
        assert!(sim.is_on_rope(), "grabbed again once the lock expired");
    }

    #[test]
    fn running_into_a_rope_carries_momentum_into_the_swing() {
        let world = rope_world();
        let mut sim = Simulation::new(Pos2::new(ROPE_ANCHOR.x - 20.0, ROPE_ANCHOR.y + 200.0));
        sim.vel = Vec2::new(cfg::PLAYER_RUN_SPEED, 0.0);
        sim.step(&world, UP, DT);
        assert!(sim.is_on_rope());
        let (_, hold) = sim.rope_hold().unwrap();
        let expect = cfg::PLAYER_RUN_SPEED / hold;
        let w = sim.rope_swing(0).ang_vel;
        assert!((w - expect).abs() < expect * 0.1, "ang_vel {w} vs v/L {expect}");
    }

    #[test]
    fn a_wall_in_the_swing_path_stops_the_swing() {
        let mut world = rope_world();
        // A wall just to the right of the hanging position.
        world.polys.push(rect(ROPE_ANCHOR.x + 60.0, 0.0, ROPE_ANCHOR.x + 400.0, 1000.0, false));
        let (mut sim, _) = hanging(200.0);
        sim.rope_swings[0] = RopeSwing { angle: 0.0, ang_vel: 3.0 };
        run_for(&mut sim, &world, none(), 1.0);
        assert!(sim.is_on_rope(), "still on the rope");
        assert!(sim.pos.x <= ROPE_ANCHOR.x + 60.0 - cfg::PLAYER_CAPSULE_RADIUS + 1.0, "pushed out of the wall");
        assert!(sim.rope_swing(0).ang_vel.abs() < 0.5, "the swing stopped");
    }

    #[test]
    fn being_hurt_drops_the_rope_and_dash_cannot_start_on_it() {
        let (mut sim, world) = hanging(200.0);
        let dash = Input { dash: true, right: true, ..NONE_INPUT };
        run_for(&mut sim, &world, dash, 0.3);
        assert!(sim.is_on_rope() && !sim.is_dashing(), "no dash from the rope");
        sim.apply_knockback(Vec2::new(300.0, -170.0), 0.3);
        sim.lock_control(0.45);
        assert!(!sim.is_on_rope(), "knockback drops the rope");
        assert_eq!(sim.state, State::Airborne);
    }

    #[test]
    fn respawn_resets_every_rope_to_hanging_still() {
        let (mut sim, world) = hanging(200.0);
        sim.rope_swings[0] = RopeSwing { angle: 1.0, ang_vel: 1.0 };
        sim.respawn();
        sim.step(&world, none(), DT);
        assert!(!sim.is_on_rope());
        assert_eq!(sim.rope_swing(0), RopeSwing::default());
    }

    #[test]
    fn convex_hull_of_a_concave_polygon_is_its_hull() {
        // Square with one vertex pushed inward; the hull drops it.
        let pts = [
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(50.0, 50.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(0.0, 100.0),
        ];
        let p = CollisionPoly::new(&pts, false).unwrap();
        assert_eq!(p.points.len(), 4, "hull should be the outer square: {:?}", p.points);
        assert!(!p.points.iter().any(|q| (q.x - 50.0).abs() < 1e-3 && (q.y - 50.0).abs() < 1e-3));
    }

    #[test]
    fn degenerate_polygon_is_rejected() {
        assert!(CollisionPoly::new(&[Pos2::ZERO, Pos2::new(1.0, 1.0)], false).is_none());
        assert!(CollisionPoly::new(&[], false).is_none());
    }

    #[test]
    fn capsule_outside_a_polygon_does_not_overlap() {
        let r = rect(0.0, 0.0, 100.0, 100.0, false);
        // Far to the left, clear of the capsule radius.
        assert!(capsule_poly_mtv(Pos2::new(-200.0, 50.0), 20.0, 25.0, &r.points).is_none());
    }

    #[test]
    fn capsule_overlapping_a_polygon_is_pushed_out_along_the_shallowest_axis() {
        let r = rect(0.0, 0.0, 100.0, 100.0, false);
        // Just above the top edge, overlapping by 5 px.
        let c = Pos2::new(50.0, 0.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y + 5.0);
        let mtv = capsule_poly_mtv(c, 20.0, 25.0, &r.points).expect("overlaps");
        assert!(mtv.y < 0.0, "should push up, got {mtv:?}");
        assert!((mtv.length() - 5.0).abs() < 0.5, "depth {:?}", mtv.length());
    }

    #[test]
    fn character_lands_on_a_polygon_surface() {
        let (s, _) = standing();
        let expected = 500.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y;
        assert!((s.pos.y - expected).abs() < 2.0, "rest y {:?}, expected ~{expected}", s.pos.y);
        assert_eq!(s.state, State::Grounded);
    }

    #[test]
    fn does_not_tunnel_through_the_floor_at_max_fall_speed() {
        let w = floor_world();
        let mut s = Simulation::new(Pos2::new(0.0, 0.0));
        run_for(&mut s, &w, none(), 3.0);
        assert!(s.vel.y.abs() < 1.0, "should be at rest, vy {:?}", s.vel.y);
        assert!(s.pos.y < 500.0, "must not end up inside/below the floor: {:?}", s.pos.y);
        assert!(s.grounded);
    }

    #[test]
    fn fall_speed_is_capped_at_max_fall_speed() {
        let w = World::default(); // No floor: fall forever.
        let mut s = Simulation::new(Pos2::ZERO);
        run_for(&mut s, &w, none(), 5.0);
        assert!(
            s.vel.y <= cfg::MAX_FALL_SPEED + 0.5,
            "terminal velocity exceeded: {:?}",
            s.vel.y
        );
        assert!(s.vel.y > cfg::MAX_FALL_SPEED - 1.0, "should have reached terminal: {:?}", s.vel.y);
    }

    #[test]
    fn holding_down_falls_faster() {
        let w = World::default();
        let mut plain = Simulation::new(Pos2::ZERO);
        let mut fast = Simulation::new(Pos2::ZERO);
        let down = Input { down: true, ..none() };
        // Short window, before either hits terminal velocity.
        run_for(&mut plain, &w, none(), 0.2);
        run_for(&mut fast, &w, down, 0.2);
        let ratio = fast.vel.y / plain.vel.y;
        assert!(
            (ratio - cfg::FAST_FALL_GRAVITY_MULT).abs() < 0.05,
            "fast-fall ratio {ratio}, expected {}",
            cfg::FAST_FALL_GRAVITY_MULT
        );
    }

    #[test]
    fn run_saturates_at_run_speed_in_about_point_one_three_seconds() {
        let (mut s, w) = standing();
        let right = Input { right: true, ..none() };
        run_for(&mut s, &w, right, cfg::PLAYER_RUN_SPEED / cfg::PLAYER_GROUND_ACCEL);
        assert!(
            (s.vel.x - cfg::PLAYER_RUN_SPEED).abs() < 15.0,
            "expected ~{} after 0.133 s, got {}",
            cfg::PLAYER_RUN_SPEED,
            s.vel.x
        );
        run_for(&mut s, &w, right, 1.0);
        assert!(
            (s.vel.x - cfg::PLAYER_RUN_SPEED).abs() < 1.0,
            "must saturate at {}, got {}",
            cfg::PLAYER_RUN_SPEED,
            s.vel.x
        );
    }

    #[test]
    fn full_hold_jump_reaches_the_configured_apex() {
        let (mut s, w) = standing();
        let start_y = s.pos.y;
        let jump = Input { jump: true, ..none() };
        let mut apex = start_y;
        for _ in 0..240 {
            s.step(&w, jump, DT);
            apex = apex.min(s.pos.y);
        }
        let height = start_y - apex;
        let tol = cfg::JUMP_HEIGHT * 0.1;
        assert!(
            (height - cfg::JUMP_HEIGHT).abs() < tol,
            "apex {height} px, expected {} ±{tol}",
            cfg::JUMP_HEIGHT
        );
    }

    #[test]
    fn releasing_jump_early_gives_a_lower_hop() {
        let (base, w) = standing();
        let start_y = base.pos.y;
        let jump = Input { jump: true, ..none() };

        let mut full = base.clone();
        let mut full_apex = start_y;
        for _ in 0..240 {
            full.step(&w, jump, DT);
            full_apex = full_apex.min(full.pos.y);
        }

        let mut tap = base.clone();
        let mut tap_apex = start_y;
        for i in 0..240 {
            tap.step(&w, if i < 6 { jump } else { none() }, DT);
            tap_apex = tap_apex.min(tap.pos.y);
        }

        let (full_h, tap_h) = (start_y - full_apex, start_y - tap_apex);
        assert!(tap_h < full_h * 0.6, "tap {tap_h} should be well under full {full_h}");
        assert!(tap_h > 0.0, "a tap must still leave the ground");
    }

    #[test]
    fn coyote_time_allows_a_jump_just_after_leaving_a_ledge() {
        // Ledge ends at x = 0; walking right steps off it.
        let w = World { polys: vec![rect(-500.0, 500.0, 0.0, 900.0, false)], ropes: vec![] };
        let jump = Input { jump: true, ..none() };

        let settle = |sim: &mut Simulation| {
            let mut s0 = Simulation::new(Pos2::new(-100.0, 400.0));
            run_for(&mut s0, &w, none(), 1.0);
            *sim = s0;
        };

        let right = Input { right: true, ..none() };
        // Walk until the ledge runs out.
        let walk_off = |sim: &mut Simulation| {
            let mut n = 0;
            while sim.grounded && n < 2000 {
                sim.step(&w, right, DT);
                n += 1;
            }
            assert!(!sim.grounded, "should have left the ledge");
        };

        // Just inside the window: the jump is allowed.
        let mut a = Simulation::new(Pos2::ZERO);
        settle(&mut a);
        walk_off(&mut a);
        run_for(&mut a, &w, none(), cfg::COYOTE_TIME * 0.5);
        a.step(&w, jump, DT);
        assert!(a.vel.y < 0.0, "coyote jump should give upward velocity, vy {:?}", a.vel.y);

        // Past the window: no jump, still falling.
        let mut b = Simulation::new(Pos2::ZERO);
        settle(&mut b);
        walk_off(&mut b);
        run_for(&mut b, &w, none(), cfg::COYOTE_TIME + 0.05);
        let vy = b.vel.y;
        b.step(&w, jump, DT);
        assert!(b.vel.y > 0.0 && b.vel.y >= vy - 1.0, "must not jump after coyote expired");
    }

    #[test]
    fn jump_buffer_fires_on_landing() {
        let w = floor_world();
        let mut s = Simulation::new(Pos2::new(0.0, 400.0));
        // Press jump while still falling, within the buffer window of the landing.
        let jump = Input { jump: true, ..none() };
        // Fall until just above the floor.
        while s.pos.y < 500.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y - 30.0 {
            s.step(&w, none(), DT);
        }
        s.step(&w, jump, DT); // one press, then release
        // Within the buffer window the press must still be honoured once we touch down.
        let mut fired = false;
        for _ in 0..((cfg::JUMP_BUFFER_TIME / DT).ceil() as usize + 2) {
            s.step(&w, none(), DT);
            if s.vel.y < 0.0 {
                fired = true;
                break;
            }
        }
        assert!(fired, "buffered jump should fire on landing, vy {:?}", s.vel.y);
    }

    #[test]
    fn coyote_window_is_exactly_coyote_time_at_its_boundaries() {
        let w = World { polys: vec![rect(-500.0, 500.0, 0.0, 900.0, false)], ropes: vec![] };
        let jump = Input { jump: true, ..none() };
        let right = Input { right: true, ..none() };

        // Step off the ledge, wait `wait` seconds, then press jump once.
        let jumps_after = |wait: f32| -> bool {
            let mut s = Simulation::new(Pos2::new(-100.0, 400.0));
            run_for(&mut s, &w, none(), 1.0);
            assert!(s.grounded);
            let mut n = 0;
            while s.grounded && n < 2000 {
                s.step(&w, right, DT);
                n += 1;
            }
            run_for(&mut s, &w, none(), wait);
            let before = s.vel.y;
            s.step(&w, jump, DT);
            s.vel.y < 0.0 && s.vel.y < before
        };

        // One step inside the window still jumps; one step past it does not.
        assert!(jumps_after(cfg::COYOTE_TIME - DT * 2.0), "must jump just inside 0.12 s");
        assert!(!jumps_after(cfg::COYOTE_TIME + DT * 2.0), "must not jump just past 0.12 s");
    }

    #[test]
    fn jump_buffer_honours_presses_inside_the_window_and_forgets_older_ones() {
        let w = floor_world();
        let jump = Input { jump: true, ..none() };

        // Press jump `lead` seconds before touching down, then release and coast in.
        let fires_with_lead = |lead: f32| -> bool {
            let mut s = Simulation::new(Pos2::new(0.0, 0.0));
            // Fall until the remaining time to the floor is about `lead`.
            loop {
                let gap = (500.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y) - s.pos.y;
                if s.vel.y > 1.0 && gap / s.vel.y <= lead {
                    break;
                }
                s.step(&w, none(), DT);
                if s.grounded {
                    return false;
                }
            }
            s.step(&w, jump, DT);
            for _ in 0..((lead / DT).ceil() as usize + 4) {
                s.step(&w, none(), DT);
                if s.vel.y < 0.0 {
                    return true;
                }
            }
            false
        };

        assert!(
            fires_with_lead(cfg::JUMP_BUFFER_TIME * 0.7),
            "a press inside the 0.1 s buffer must fire on landing"
        );
        assert!(
            !fires_with_lead(cfg::JUMP_BUFFER_TIME * 3.0),
            "a press well before the buffer window must be forgotten"
        );
    }

    #[test]
    fn wall_coyote_allows_a_wall_jump_just_after_leaving_the_wall() {
        let w = wall_world(false);
        let start = Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0);

        let jumps_after = |wait: f32| -> bool {
            let mut s = Simulation::new(start);
            run_for(&mut s, &w, Input { right: true, ..none() }, 0.5);
            assert_ne!(s.wall_dir, 0.0, "should be on the wall first");
            // Peel away from the wall, then wait.
            run_for(&mut s, &w, Input { left: true, ..none() }, DT * 3.0);
            run_for(&mut s, &w, none(), wait);
            s.step(&w, Input { jump: true, ..none() }, DT);
            s.vel.y < 0.0
        };

        assert!(jumps_after(0.0), "a wall jump right after leaving the wall is allowed");
        assert!(
            !jumps_after(cfg::WALL_COYOTE_TIME + 0.05),
            "past WALL_COYOTE_TIME the wall jump must be refused"
        );
    }

    #[test]
    fn dash_covers_the_configured_distance_at_dash_speed() {
        let (mut s, w) = standing();
        let x0 = s.pos.x;
        let dash = Input { right: true, dash: true, ..none() };
        s.step(&w, dash, DT);
        assert_eq!(s.state, State::Dashing);
        let mut peak = 0.0_f32;
        for _ in 0..((cfg::DASH_DISTANCE / cfg::DASH_SPEED / DT).round() as usize) {
            peak = peak.max(s.vel.x.abs());
            s.step(&w, Input { right: true, ..none() }, DT);
        }
        let travelled = s.pos.x - x0;
        assert!(
            (travelled - cfg::DASH_DISTANCE).abs() < cfg::DASH_DISTANCE * 0.1,
            "dash travelled {travelled}, expected ~{}",
            cfg::DASH_DISTANCE
        );
        assert!(
            (peak - cfg::DASH_SPEED).abs() < 1.0,
            "peak dash speed {peak}, expected {}",
            cfg::DASH_SPEED
        );
    }

    #[test]
    fn dash_respects_its_cooldown() {
        let (mut s, w) = standing();
        let dash = Input { right: true, dash: true, ..none() };
        s.step(&w, dash, DT);
        run_for(&mut s, &w, Input { right: true, ..none() }, cfg::DASH_DISTANCE / cfg::DASH_SPEED);
        // Immediately press again: refused at that instant because the cooldown is live.
        s.step(&w, dash, DT);
        assert_ne!(s.state, State::Dashing, "second dash must wait for the cooldown");
        // That press stays buffered for DASH_BUFFER_TIME and fires as soon as the
        // cooldown clears, which restarts the cooldown; wait the whole cycle out.
        run_for(
            &mut s,
            &w,
            none(),
            cfg::DASH_BUFFER_TIME + cfg::DASH_DISTANCE / cfg::DASH_SPEED + cfg::DASH_COOLDOWN,
        );
        assert_ne!(s.state, State::Dashing, "the buffered dash should have finished");
        s.step(&w, dash, DT);
        assert_eq!(s.state, State::Dashing, "dash should be available once fully off cooldown");
    }

    #[test]
    fn exactly_one_air_dash_is_allowed_per_airtime() {
        let w = World::default(); // Airborne forever.
        let mut s = Simulation::new(Pos2::ZERO);
        let dash = Input { right: true, dash: true, ..none() };
        s.step(&w, dash, DT);
        assert_eq!(s.state, State::Dashing, "first air dash should start");
        run_for(&mut s, &w, none(), cfg::DASH_DISTANCE / cfg::DASH_SPEED + cfg::DASH_COOLDOWN + 0.1);
        s.step(&w, dash, DT);
        assert_ne!(s.state, State::Dashing, "second air dash must be refused");
    }

    #[test]
    fn landing_restores_the_air_dash() {
        let (mut s, w) = standing();
        // Jump, dash in the air, land, then dash again in the air.
        run_for(&mut s, &w, Input { jump: true, ..none() }, 0.1);
        s.step(&w, Input { right: true, dash: true, jump: true, ..none() }, DT);
        assert_eq!(s.state, State::Dashing);
        run_for(&mut s, &w, none(), 3.0); // land and settle
        assert!(s.grounded, "should be back on the ground");
        run_for(&mut s, &w, Input { jump: true, ..none() }, 0.1);
        s.step(&w, Input { right: true, dash: true, jump: true, ..none() }, DT);
        assert_eq!(s.state, State::Dashing, "landing must reset the air dash");
    }

    /// Floor plus a tall wall on the right at x = 200.
    fn wall_world(climbable: bool) -> World {
        World {
            polys: vec![
                rect(-5000.0, 500.0, 5000.0, 900.0, false),
                rect(200.0, -500.0, 400.0, 500.0, climbable),
            ],
            ropes: vec![],
        }
    }

    #[test]
    fn wall_slide_caps_the_descent_speed() {
        let w = wall_world(false);
        // Start high, next to the wall, pressing into it.
        let mut s = Simulation::new(Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0));
        let right = Input { right: true, ..none() };
        run_for(&mut s, &w, right, 1.0);
        assert_ne!(s.wall_dir, 0.0, "should be touching the wall");
        assert!(
            s.vel.y <= cfg::WALL_SLIDE_MAX_FALL_SPEED + 1.0,
            "wall slide descent {} should be capped at {}",
            s.vel.y,
            cfg::WALL_SLIDE_MAX_FALL_SPEED
        );
        assert_eq!(s.state, State::WallSliding);
    }

    #[test]
    fn wall_jump_launches_up_and_away_with_an_input_lock() {
        let w = wall_world(false);
        let mut s = Simulation::new(Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0));
        let right = Input { right: true, ..none() };
        run_for(&mut s, &w, right, 0.5);
        assert_ne!(s.wall_dir, 0.0, "should be on the wall before jumping");

        let start_y = s.pos.y;
        let start_x = s.pos.x;
        let held = Input { jump: true, ..none() };
        s.step(&w, held, DT);
        assert!(s.vel.y < 0.0, "wall jump should go up, vy {}", s.vel.y);
        assert!(s.vel.x < 0.0, "wall jump should go away from the wall, vx {}", s.vel.x);

        // Keep jump held for the full-height arc (releasing shortens it, as in the game)
        // and give no horizontal input, so the arc keeps its launch velocity.
        let mut apex = start_y;
        let mut steps = 0;
        while s.vel.y < 0.0 && steps < 400 {
            s.step(&w, held, DT);
            apex = apex.min(s.pos.y);
            steps += 1;
        }
        let height = start_y - apex;
        assert!(
            (height - cfg::WALL_JUMP_HEIGHT).abs() < cfg::WALL_JUMP_HEIGHT * 0.15,
            "wall jump apex {height}, expected ~{}",
            cfg::WALL_JUMP_HEIGHT
        );
        // Fall back to the takeoff height and check the horizontal carry.
        while s.pos.y < start_y && steps < 800 {
            s.step(&w, held, DT);
            steps += 1;
        }
        let carried = (start_x - s.pos.x).abs();
        assert!(
            (carried - cfg::WALL_JUMP_HORIZONTAL_DISTANCE).abs()
                < cfg::WALL_JUMP_HORIZONTAL_DISTANCE * 0.2,
            "wall jump carried {carried}, expected ~{}",
            cfg::WALL_JUMP_HORIZONTAL_DISTANCE
        );
    }

    #[test]
    fn horizontal_input_is_ignored_during_the_wall_jump_lock() {
        let w = wall_world(false);
        let mut s = Simulation::new(Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0));
        run_for(&mut s, &w, Input { right: true, ..none() }, 0.5);
        s.step(&w, Input { jump: true, ..none() }, DT);
        let vx = s.vel.x;
        // Push back into the wall during the lock: velocity must not respond.
        run_for(&mut s, &w, Input { right: true, ..none() }, cfg::WALL_JUMP_INPUT_LOCK * 0.5);
        assert!(
            (s.vel.x - vx).abs() < 1.0,
            "input lock should hold vx at {vx}, got {}",
            s.vel.x
        );
    }

    #[test]
    fn climbs_a_wall_tool_polygon_at_climb_speed() {
        let w = wall_world(true);
        let mut s = Simulation::new(Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0));
        let into = Input { right: true, ..none() };
        run_for(&mut s, &w, into, 0.5);
        assert_ne!(s.wall_dir, 0.0, "should be on the wall");

        let up = Input { right: true, up: true, ..none() };
        let y0 = s.pos.y;
        run_for(&mut s, &w, up, 0.5);
        assert_eq!(s.state, State::Climbing);
        let climbed = y0 - s.pos.y;
        assert!(
            (climbed - cfg::CLIMB_SPEED * 0.5).abs() < cfg::CLIMB_SPEED * 0.5 * 0.2,
            "climbed {climbed} in 0.5 s, expected ~{}",
            cfg::CLIMB_SPEED * 0.5
        );

        let down = Input { right: true, down: true, ..none() };
        let y1 = s.pos.y;
        run_for(&mut s, &w, down, 0.3);
        assert!(s.pos.y > y1, "down should descend the wall");
    }

    #[test]
    fn does_not_climb_a_plain_polygon() {
        let w = wall_world(false);
        let mut s = Simulation::new(Pos2::new(200.0 - cfg::PLAYER_CAPSULE_RADIUS - 2.0, 0.0));
        run_for(&mut s, &w, Input { right: true, ..none() }, 0.5);
        let y0 = s.pos.y;
        run_for(&mut s, &w, Input { right: true, up: true, ..none() }, 0.5);
        assert_ne!(s.state, State::Climbing, "a non-wall_tool polygon must not be climbable");
        assert!(s.pos.y >= y0 - 1.0, "must not gain height on a plain polygon");
    }

    #[test]
    fn trajectory_is_identical_at_60hz_and_144hz() {
        let w = floor_world();
        let input = Input { right: true, jump: true, ..none() };
        let mut a = Simulation::new(Pos2::new(0.0, 300.0));
        let mut b = Simulation::new(Pos2::new(0.0, 300.0));
        for _ in 0..120 {
            a.advance(&w, input, 1.0 / 60.0);
        }
        for _ in 0..288 {
            b.advance(&w, input, 1.0 / 144.0);
        }
        // Both advanced 2 s of simulated time. An accumulator can be one fixed step
        // ahead or behind at the sampling instant, so allow a single step of travel at
        // run speed rather than exact equality.
        let tol = cfg::PLAYER_RUN_SPEED * FIXED_DT * 1.5;
        assert!(
            (a.pos.x - b.pos.x).abs() < tol && (a.pos.y - b.pos.y).abs() < tol,
            "60 Hz {:?} vs 144 Hz {:?} differ by more than one step ({tol:.1} px)",
            a.pos,
            b.pos
        );
    }

    #[test]
    fn advance_ignores_invalid_dt_and_bounds_catch_up() {
        let w = floor_world();
        let mut s = Simulation::new(Pos2::new(0.0, 300.0));
        assert_eq!(s.advance(&w, none(), f32::NAN), 0);
        assert_eq!(s.advance(&w, none(), -1.0), 0);
        assert_eq!(s.advance(&w, none(), 0.0), 0);
        // A huge stall must not run unbounded steps.
        assert_eq!(s.advance(&w, none(), 10.0), MAX_STEPS_PER_FRAME);
    }

    #[test]
    fn respawn_resets_position_and_motion() {
        let (mut s, w) = standing();
        run_for(&mut s, &w, Input { right: true, jump: true, ..none() }, 0.4);
        assert!(s.vel.length() > 0.0);
        s.respawn();
        assert_eq!(s.pos, s.spawn);
        assert_eq!(s.vel, Vec2::ZERO);

        let p = Pos2::new(1234.0, 56.0);
        s.respawn_at(p);
        assert_eq!(s.pos, p);
        assert_eq!(s.spawn, p);
    }

    #[test]
    fn state_labels_are_stable() {
        assert_eq!(State::Grounded.label(), "run");
        assert_eq!(State::Airborne.label(), "air");
        assert_eq!(State::Dashing.label(), "dash");
        assert_eq!(State::WallSliding.label(), "wall slide");
        assert_eq!(State::Climbing.label(), "climb");
    }

    #[test]
    fn opposite_horizontal_inputs_cancel() {
        let (mut s, w) = standing();
        run_for(&mut s, &w, Input { left: true, right: true, ..none() }, 0.5);
        assert!(s.vel.x.abs() < 1.0, "left+right should cancel, vx {}", s.vel.x);
    }
}
