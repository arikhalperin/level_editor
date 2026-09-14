//! Character tuning transcribed from the game this editor builds levels for.
//!
//! Source: `/Users/arikha/bevy_prince_platformer/src/game/config.rs`.
//! Names are kept identical to the game's so drift between the two is auditable by
//! diffing the two files. Only the constants transfer: the game drives them through
//! `bevy-tnua` 0.31 on `bevy_rapier2d` 0.33, a physics-engine floating character
//! controller that cannot run inside an egui app, so [`crate::sim`] is an independent
//! kinematic implementation of the same numbers.
//!
//! Units are world pixels and seconds throughout, as in the game.

#![allow(dead_code)] // Constants are transcribed as a set; not all are used yet.

/// The game flips editor Y with this constant. A level whose height is not 720 will be
/// vertically offset there until the game reads the height from the level JSON instead.
/// Recorded here as documentation; the editor itself never flips Y.
pub const LEVEL_JSON_VIEWPORT_HEIGHT: f32 = 720.0;

pub const GRAVITY: f32 = 1400.0;

// ── Player body ─────────────────────────────────────────────────────────────────
pub const PLAYER_CAPSULE_HALF_HEIGHT: f32 = 20.0;
pub const PLAYER_CAPSULE_RADIUS: f32 = 25.0;
pub const PLAYER_CAPSULE_HALF_EXTENT_Y: f32 = PLAYER_CAPSULE_HALF_HEIGHT + PLAYER_CAPSULE_RADIUS;

// ── Run ─────────────────────────────────────────────────────────────────────────
pub const PLAYER_RUN_SPEED: f32 = 600.0;
pub const PLAYER_GROUND_ACCEL: f32 = 4500.0;
pub const PLAYER_AIR_ACCEL: f32 = 3200.0;
pub const COYOTE_TIME: f32 = 0.12;
pub const PLAYER_FREE_FALL_EXTRA_GRAVITY: f32 = 500.0;
pub const FAST_FALL_GRAVITY_MULT: f32 = 1.8;
pub const MAX_FALL_SPEED: f32 = 950.0;

// ── Jump ────────────────────────────────────────────────────────────────────────
pub const JUMP_HEIGHT: f32 = 365.0;
pub const JUMP_SHORTEN_EXTRA_GRAVITY: f32 = 9000.0;
pub const JUMP_FALL_EXTRA_GRAVITY: f32 = 900.0;
pub const JUMP_BUFFER_TIME: f32 = 0.1;
pub const MAX_AIR_JUMPS: usize = 0;

// ── Dash ────────────────────────────────────────────────────────────────────────
pub const DASH_SPEED: f32 = 1500.0;
pub const DASH_DISTANCE: f32 = 450.0;
pub const DASH_BRAKE_TO_SPEED: f32 = PLAYER_RUN_SPEED;
pub const DASH_COOLDOWN: f32 = 0.4;
pub const MAX_AIR_DASHES: usize = 1;
pub const DASH_BUFFER_TIME: f32 = 0.1;

// ── Wall slide / wall jump ──────────────────────────────────────────────────────
pub const WALL_SENSOR_REACH: f32 = PLAYER_CAPSULE_RADIUS + 10.0;
pub const WALL_SLIDE_MAX_FALL_SPEED: f32 = 110.0;
pub const WALL_JUMP_HEIGHT: f32 = 190.0;
pub const WALL_JUMP_HORIZONTAL_DISTANCE: f32 = 170.0;
pub const WALL_JUMP_INPUT_LOCK: f32 = 0.18;
pub const WALL_COYOTE_TIME: f32 = 0.1;

// ── Climb (level `wall_tool` polygons) ──────────────────────────────────────────
pub const CLIMB_SPEED: f32 = 140.0;
pub const CLIMB_WALL_DRIFT: f32 = 60.0;

// ── Camera ──────────────────────────────────────────────────────────────────────
pub const CAMERA_FOLLOW_RATE_X: f32 = 7.0;
pub const CAMERA_FOLLOW_RATE_Y: f32 = 5.0;
pub const CAMERA_FALL_CATCHUP_MULT: f32 = 2.5;
pub const CAMERA_FALL_SPEED_THRESHOLD: f32 = 400.0;
pub const CAMERA_DEAD_ZONE_X: f32 = 40.0;
pub const CAMERA_OFFSET_Y: f32 = 80.0;

// ── Katana ──────────────────────────────────────────────────────────────────────
pub const ATTACK_INPUT_BUFFER_SEC: f32 = 0.12;
pub const SLASH_COOLDOWN: f32 = 0.28;
pub const SLASH_GROUND_SPEED_MULT: f32 = 0.35;
pub const SLASH_FORWARD_HALF_W: f32 = 62.0;
pub const SLASH_FORWARD_HALF_H: f32 = 50.0;
pub const SLASH_FORWARD_REACH: f32 = 54.0;
pub const SLASH_FORWARD_Y: f32 = 8.0;
pub const SLASH_VERTICAL_HALF_W: f32 = 50.0;
pub const SLASH_VERTICAL_HALF_H: f32 = 44.0;
pub const SLASH_VERTICAL_REACH: f32 = 62.0;
pub const POGO_HEIGHT: f32 = 160.0;
pub const SLASH_RECOIL_SPEED: f32 = 400.0;
pub const SLASH_RECOIL_SECS: f32 = 0.1;

// Slash timing lives in the game's animation clip table (`sugisan/animation.rs`), not
// its config, so it is derived here from the same frame counts and rates. Forward:
// 15 frames at 60 fps, blade live over frames 5..=8. Up / down: 8 frames at 32 fps,
// live over frames 2..=5. `hit_window` ends at the start of the frame after the last
// live one, matching the game's `AnimationClip::hit_window`.
pub const SLASH_FORWARD_DURATION: f32 = 15.0 / 60.0;
pub const SLASH_FORWARD_HIT_FROM: f32 = 5.0 / 60.0;
pub const SLASH_FORWARD_HIT_TO: f32 = 9.0 / 60.0;
pub const SLASH_VERTICAL_DURATION: f32 = 8.0 / 32.0;
pub const SLASH_VERTICAL_HIT_FROM: f32 = 2.0 / 32.0;
pub const SLASH_VERTICAL_HIT_TO: f32 = 6.0 / 32.0;

// ── Player condition ────────────────────────────────────────────────────────────
pub const INITIAL_HEALTH: i32 = 3;
pub const HURT_LOCKOUT_SECS: f32 = 0.45;
pub const PLAYER_CLUB_IFRAMES: f32 = 1.3;
pub const SHIELD_ACTIVE_SECS: f32 = 2.0;
pub const SHIELD_COOLDOWN_SECS: f32 = 5.0;

// ── Orc ─────────────────────────────────────────────────────────────────────────
/// Fixed capsule in the game's `orc/spawn.rs`, independent of the level sprite size.
pub const ORC_CAPSULE_HALF_HEIGHT: f32 = 38.0;
pub const ORC_CAPSULE_RADIUS: f32 = 32.0;
pub const ORC_WALL_STOP_DISTANCE_PX: f32 = 50.0;
pub const ORC_MIN_MOVING_SPEED: f32 = 15.0;
pub const ORC_GROUND_PROBE_FEET_OFFSET_Y: f32 = 68.0;
pub const ORC_GROUND_PROBE_MAX_DIST: f32 = 96.0;
pub const ORC_LEDGE_PROBE_FORWARD_PX: f32 = 52.0;
pub const ORC_ATTACK_RANGE_PX: f32 = 105.0;
pub const ORC_ATTACK_RANGE_LEAVE_PX: f32 = 160.0;
/// Beyond this the orc stays Idle (`behavior.rs`: `distance < 400.0`).
pub const ORC_CHARGE_TRIGGER_PX: f32 = 400.0;
/// `behavior.rs`: `PrepareChargeTimer` wind-up, 0 when recovering from a sword freeze.
pub const ORC_PREPARE_CHARGE_SECS: f32 = 1.0;
/// `behavior.rs`: initial charge velocity, acceleration and horizontal cap.
pub const ORC_CHARGE_LAUNCH_SPEED: f32 = 200.0;
pub const ORC_CHARGE_ACCEL: f32 = 1000.0;
pub const ORC_CHARGE_MAX_SPEED: f32 = 250.0;

pub const ORC_MAX_POWER: f32 = 100.0;
pub const ORC_SWORD_POWER_FRACTION: f32 = 0.25;
pub const ORC_DEFEAT_SCORE: i32 = 100;
pub const ORC_SWORD_KNOCKBACK_X: f32 = 260.0;
pub const ORC_SWORD_KNOCKBACK_Y: f32 = 120.0;
pub const ORC_SWORD_KNOCKBACK_SECS: f32 = 0.3;
pub const ORC_HIT_FREEZE_SEC: f32 = 0.35;
/// How long after a sword freeze the orc still counts as "already in the fight" and
/// skips its charge wind-up (`orc/components.rs: ORC_FREEZE_RECOVERED_SECS`).
pub const ORC_FREEZE_RECOVERED_SECS: f32 = 0.5;

pub const ORC_CLUB_HIT_MAX_SEPARATION_X_PX: f32 = 112.0;
pub const ORC_CLUB_HIT_MAX_SEPARATION_Y_PX: f32 = 82.0;
pub const ORC_CLUB_DAMAGE: i32 = 1;
pub const ORC_CLUB_KNOCKBACK_X: f32 = 300.0;
pub const ORC_CLUB_KNOCKBACK_Y: f32 = 170.0;
pub const ORC_REATTACK_DELAY_SEC: f32 = 0.45;
pub const ORC_ATTACKING_ANIM_FPS: f32 = 12.0;
pub const ORC_ANIM_STRIP_FRAME_COUNT: u32 = 10;
pub const ORC_CLUB_ACTIVE_FRAME_MIN: usize = 3;
pub const ORC_CLUB_ACTIVE_FRAME_MAX: usize = 5;

// The club's damaging window is expressed in animation frames in the game; this
// simulation has no sprites, so it is derived from the same frames and playback rate:
// one swing is 10 frames at 12 fps, and the club bites over frames 3..=5.
pub const ORC_SWING_DURATION: f32 =
    ORC_ANIM_STRIP_FRAME_COUNT as f32 / ORC_ATTACKING_ANIM_FPS;
pub const ORC_CLUB_ACTIVE_FROM: f32 =
    ORC_CLUB_ACTIVE_FRAME_MIN as f32 / ORC_ATTACKING_ANIM_FPS;
pub const ORC_CLUB_ACTIVE_TO: f32 =
    (ORC_CLUB_ACTIVE_FRAME_MAX as f32 + 1.0) / ORC_ATTACKING_ANIM_FPS;

// ── Pickups and hazards ─────────────────────────────────────────────────────────
pub const COIN_PICKUP_POINTS: i32 = 40;
pub const COIN_LEVEL_SIZE_SCALE: f32 = 0.7;
pub const DEATH_PIT_SIZE_SCALE: f32 = 4.0;

/// Level `bitmap_name`s the game's importer turns into behaviour (`level1.rs`).
pub const ORC_BITMAP: &str = "orc_tool";
pub const COIN_BITMAP: &str = "coin_tool";
pub const DEATH_PIT_BITMAP: &str = "death_trap_tool";

/// The polygon type the game treats as a climbable wall
/// (`world_objects::spawn_polygon_collider`: `is_wall = polygon_type == "wall_tool"`).
pub const CLIMBABLE_POLYGON_TYPE: &str = "wall_tool";

// ── Rope ────────────────────────────────────────────────────────────────────────
// Defined here first: the editor's `sim` is the reference implementation of the rope
// swing and the game reproduces these numbers (see the `rope-swing` spec).

/// Level `bitmap_name` the importers turn into a rope. The entry's `position` is the
/// anchor and its `size` is `[ROPE_THICKNESS, length]`.
pub const ROPE_BITMAP: &str = "rope_tool";
/// Saved `size.x` and drawn line width, px.
pub const ROPE_THICKNESS: f32 = 6.0;
/// Shortest rope the tool creates or the loader accepts, px.
pub const ROPE_MIN_LENGTH: f32 = 60.0;
/// Closest the hold point gets to the anchor, px.
pub const ROPE_MIN_HOLD: f32 = 40.0;
/// Max distance from the character's centre to the rope line for a grab, px.
pub const ROPE_GRAB_HALF_WIDTH: f32 = 30.0;
/// Tangential acceleration while left / right is held, px/s².
pub const ROPE_PUMP_ACCEL: f32 = 900.0;
/// Angular-velocity decay rate, 1/s.
pub const ROPE_DAMPING: f32 = 0.35;
/// The swing never passes this angle from vertical, degrees.
pub const ROPE_MAX_ANGLE_DEG: f32 = 75.0;
/// Upward boost added on release, px (a wall jump's height).
pub const ROPE_RELEASE_HOP_HEIGHT: f32 = WALL_JUMP_HEIGHT;
/// No grab of any rope for this long after a release, seconds.
pub const ROPE_REGRAB_LOCK: f32 = 0.25;
