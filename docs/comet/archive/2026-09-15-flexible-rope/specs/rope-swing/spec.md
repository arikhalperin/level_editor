# Rope swing

> Amended 2026-09-14 by the flexible-rope change: the rope is now a flexible chain. The pendulum that drives the swing is unchanged; the rope's own shape, the free rope and the grab geometry are defined by the chain (see *Rope chain*).

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
| `ROPE_SEGMENT_LEN` | 16 px | Rest length of one chain link (a rope has at least 3). |
| `ROPE_CHAIN_DAMPING` | 0.985 | Per-step velocity retention of free chain links. |
| `ROPE_CHAIN_ITERATIONS` | 80 | Distance-constraint passes per step. |
| `ROPE_SETTLE_SPEED` | 2 px/s | Below this for every link, a free rope snaps to hanging straight. |
| `CLIMB_SPEED` | 140 px/s (existing) | Speed of the hold point along the rope. |
| `GRAVITY` | 1400 px/s² (existing) | Pendulum gravity. |

## Rope chain

Every rope is a chain of `n = max(3, round(length / ROPE_SEGMENT_LEN))` links of rest length `length / n`, positions `p_0 ..= p_n` with `p_0` pinned at the anchor. Chain state is play state: it starts hanging straight when play starts or restarts and is never level data.

Each fixed step, for every link that is not pinned: `v = (p − p_prev) · ROPE_CHAIN_DAMPING`, `p_prev = p`, `p += v + (0, GRAVITY) · dt²`; then `ROPE_CHAIN_ITERATIONS` passes enforce every segment's rest length (a hanging chain needs the gravity error to propagate all the way up to the anchor, which is why the count is high) by moving the two links toward or away from each other (a pinned link does not move; a free link takes the whole correction against a pinned one). A free rope is only this chain under gravity; when every link's speed is below `ROPE_SETTLE_SPEED` it snaps to hanging straight down and rests (it is not simulated again until a grab or release disturbs it).

While held (see *Swinging*), the pendulum owns the part from the anchor to the hands: angle `θ` and hold length `L` are the archived pendulum's; the links with arc length `s ≤ L` are pinned at `anchor + s · (sin θ, cos θ)` and the hand link (the one at arc length `L`, inserted or moved to that arc length) at the hold position; the links beyond the hand run free, hanging from the hand and lagging the swing.

The rope's line, for drawing and for the grab test, is the polyline through the links; the archived straight-line description is superseded.

## Grabbing

A grab happens on a simulation step when all hold:

- the character's condition is normal (not hurt, not dead) and no regrab lock is running;
- the character is not dashing;
- up or down is held (`axis_y ≠ 0`);
- the distance from the character's centre to the rope's chain polyline is at most `ROPE_GRAB_HALF_WIDTH`, measured against the part of the chain from `ROPE_MIN_HOLD` of arc length to the end.

On grab: the hold length `L` is the arc length along the chain to the nearest point, clamped to `[ROPE_MIN_HOLD, length]`; `θ` is the direction from the anchor to that point; `ω = (v · t) / L` where `v` is the character's velocity and `t = (cos θ, −sin θ)` the tangent, so momentum carries into the swing. The taut part snaps onto the straight line (tension). The character's state becomes `Rope`; the overlay label is `rope`.

## Swinging

While on the rope, each step:

- pump: `a_t = axis_x · ROPE_PUMP_ACCEL`;
- `ω̇ = −(GRAVITY / L) · sin θ + a_t / L − ROPE_DAMPING · ω`; `θ += ω · dt`;
- angle cap: if `|θ|` would exceed `ROPE_MAX_ANGLE_DEG`, `θ` is clamped to it and `ω` is set to 0 when it points outward;
- climb: `L += −axis_y · CLIMB_SPEED · dt` (up shortens), clamped to `[ROPE_MIN_HOLD, length]`;
- the character's centre is placed at `A + L · (sin θ, cos θ)` and its velocity set to `L · ω · t`; the character faces the direction of `ω` when it is non-zero.

The taut part of the chain is pinned along `θ` while held (see *Rope chain*); the tail hangs from the hands.

## Collisions and interruptions

- After placement the usual capsule-vs-polygon push-out runs. If the character was pushed, the rope's angle is recomputed from the pushed centre (`θ = atan2(dx, dy)` relative to the anchor, `L` unchanged) and `ω` is set to 0: the swing stops against the wall.
- Landing on ground does not detach (the character can stand at the rope's end while holding it); grounded contact does not change the state.
- A non-normal condition (hurt, dead), or a restart / respawn, drops the rope: state returns to airborne with the current velocity, and the chain runs free from its current shape and motion.
- Dash and wall jump cannot start while on the rope; attack and shield follow `play-combat` unchanged.

## Release

Pressing jump while on the rope releases it: state becomes airborne, velocity = `L · ω · t + (0, −jump_speed(ROPE_RELEASE_HOP_HEIGHT))` (the same `jump_speed` the simulation uses for jumps), the air dash is restored, and `ROPE_REGRAB_LOCK` starts. The chain runs free from its current shape: each formerly taut link keeps its pendulum velocity `s · ω · t`, so nothing jumps, and the rope swings and ripples on, damped.

## Presentation

While playing, each rope is drawn as a polyline through its chain links, in the same colour as on the canvas. When not playing, ropes are drawn as a straight line from the anchor to the end.

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
- A10 — Chain: every segment keeps its rest length within 2% and link 0 stays at the anchor; a displaced free rope ripples and settles to straight within 6 s.
- A11 — While held, the links up to the hand lie on the taut line at `θ` and the hand link is at the hold position; the tail hangs from the hand and lags the swing.
- A12 — The grab test follows the chain polyline; on release no link jumps more than one segment length; the chain is deterministic for identical inputs.
- A13 — In play the rope is drawn as a polyline through its links; while editing it is a straight line.
