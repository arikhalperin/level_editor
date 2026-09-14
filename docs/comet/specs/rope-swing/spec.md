# Rope swing

Complete target behaviour of ropes in the editor's play simulation. This is also the reference model for the Bevy game: the constants are declared in `game_config.rs` with the same names the game will use, and the equations below are what both implementations integrate. World space here is the editor's (Y-down); the game flips Y as it does for all level data.

## Constants (`game_config.rs`)

| Name | Value | Meaning |
| --- | --- | --- |
| `ROPE_BITMAP` | `"rope_tool"` | Level `bitmap_name` the importers turn into a rope. |
| `ROPE_THICKNESS` | 6 px | Saved `size.x` and drawn line width. |
| `ROPE_MIN_LENGTH` | 60 px | Shortest rope the tool creates or the loader accepts. |
| `ROPE_MIN_HOLD` | 40 px | Closest the hold point gets to the anchor. |
| `ROPE_GRAB_HALF_WIDTH` | 30 px | Max distance from the character's centre to the rope line for a grab. |
| `ROPE_PUMP_ACCEL` | 900 px/s² | Tangential acceleration while left / right is held. |
| `ROPE_DAMPING` | 0.35 /s | Angular-velocity decay rate. |
| `ROPE_MAX_ANGLE_DEG` | 75° | The swing never passes this angle from vertical. |
| `ROPE_RELEASE_HOP_HEIGHT` | `WALL_JUMP_HEIGHT` 190 px | Upward boost added on release. |
| `ROPE_REGRAB_LOCK` | 0.25 s | No grab of any rope for this long after a release. |
| `CLIMB_SPEED` | 140 px/s (existing) | Speed of the hold point along the rope. |
| `GRAVITY` | 1400 px/s² (existing) | Pendulum gravity. |

## Rope state

Every rope in the level has, during play, an angle `θ` from vertical (0 = hanging straight down, positive toward +x) and an angular velocity `ω`. Both start at 0 when play starts or restarts, and are not level data. The rope's line runs from its anchor `A` to `A + length · (sin θ, cos θ)`.

Free rope (nobody on it): `ω̇ = −(GRAVITY / length) · sin θ − ROPE_DAMPING · ω`, `θ̇ = ω`, integrated on the fixed step. When `|θ| < 0.5°` and `|ω| < 0.02 rad/s` the rope snaps to rest (`θ = ω = 0`).

## Grabbing

A grab happens on a simulation step when all hold:

- the character's condition is normal (not hurt, not dead) and no regrab lock is running;
- the character is not dashing;
- up or down is held (`axis_y ≠ 0`);
- the distance from the character's centre to the rope's current line segment is at most `ROPE_GRAB_HALF_WIDTH`, measured against the part of the rope from `ROPE_MIN_HOLD` below the anchor to its end.

On grab: the hold length `L` is the projection of the character's centre onto the rope's line, clamped to `[ROPE_MIN_HOLD, length]`; `θ` is the rope's current angle; `ω = (v · t) / L` where `v` is the character's velocity and `t = (cos θ, −sin θ)` the tangent, so momentum carries into the swing. The character's state becomes `Rope`; the overlay label is `rope`.

## Swinging

While on the rope, each step:

- pump: `a_t = axis_x · ROPE_PUMP_ACCEL`;
- `ω̇ = −(GRAVITY / L) · sin θ + a_t / L − ROPE_DAMPING · ω`; `θ += ω · dt`;
- angle cap: if `|θ|` would exceed `ROPE_MAX_ANGLE_DEG`, `θ` is clamped to it and `ω` is set to 0 when it points outward;
- climb: `L += −axis_y · CLIMB_SPEED · dt` (up shortens), clamped to `[ROPE_MIN_HOLD, length]`;
- the character's centre is placed at `A + L · (sin θ, cos θ)` and its velocity set to `L · ω · t`; the character faces the direction of `ω` when it is non-zero.

The rope's own `θ, ω` are the character's while held (one pendulum), so the line is drawn from the anchor through the character to the rope's end.

## Collisions and interruptions

- After placement the usual capsule-vs-polygon push-out runs. If the character was pushed, the rope's angle is recomputed from the pushed centre (`θ = atan2(dx, dy)` relative to the anchor, `L` unchanged) and `ω` is set to 0: the swing stops against the wall.
- Landing on ground does not detach (the character can stand at the rope's end while holding it); grounded contact does not change the state.
- A non-normal condition (hurt, dead), or a restart / respawn, drops the rope: state returns to airborne with the current velocity, and the rope keeps `θ, ω` and swings on freely.
- Dash and wall jump cannot start while on the rope; attack and shield follow `play-combat` unchanged.

## Release

Pressing jump while on the rope releases it: state becomes airborne, velocity = `L · ω · t + (0, −jump_speed(ROPE_RELEASE_HOP_HEIGHT))` (the same `jump_speed` the simulation uses for jumps), the air dash is restored, and `ROPE_REGRAB_LOCK` starts. The rope keeps `ω` and swings on, damped.

## Presentation

While playing, each rope is drawn from its anchor along its current angle to its end, in the same colour as on the canvas; a held rope is drawn to the character and on to the rope's end. When not playing, ropes hang straight down.

## Acceptance

- A1 — Holding up or down within 30 px of a rope's line, at least 40 px below the anchor, attaches the character at that height; the overlay reads `rope`.
- A2 — From rest at 30° with no input the swing's period is within 10% of `2π·sqrt(L / GRAVITY)`, and it settles to `θ = 0`.
- A3 — Holding right from a hanging rest builds a swing whose angle never exceeds 75°.
- A4 — Up / down move `L` at 140 px/s, bounded by 40 px and the rope's length.
- A5 — Jump releases with `L · ω · t` plus a 190 px hop; no grab is possible for 0.25 s; the rope goes on swinging.
- A6 — Entering the rope at run speed gives `ω = (v · t) / L`.
- A7 — A polygon in the swing path stops the swing; hurt or death drops the rope; dash and wall jump do not start while on it.
- A8 — Restart and stop reset every rope to hanging straight; play never changes rope entities or `has_unsaved_changes()`.
- A9 — `game_config.rs` declares the constants above with these values.
