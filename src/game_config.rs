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

/// The polygon type the game treats as a climbable wall
/// (`world_objects::spawn_polygon_collider`: `is_wall = polygon_type == "wall_tool"`).
pub const CLIMBABLE_POLYGON_TYPE: &str = "wall_tool";
