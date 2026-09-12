# Play simulation

> Amended 2026-09-12 by the play-combat-simulation change: bitmap entities are no longer inert (`orc_tool`, `coin_tool` and `death_trap_tool` gain behaviour, specified in the `play-combat` capability), the input list gained the attack, shield and debug keys, `R` restarts the whole run rather than only respawning, and the overlay gained health, score, coins and shield. Movement, the spawn point, the fixed timestep and the non-destructive guarantee are unchanged.

Complete target behaviour of the editor's in-editor play mode after this change is archived.

## Purpose and fidelity

Play mode drops a controllable character into the level currently being edited so its geometry can be tested without exporting to the game. Movement reproduces the platformer at `/Users/arikha/bevy_prince_platformer`, whose character is a `bevy-tnua` 0.31 floating controller on `bevy_rapier2d` 0.33. That controller is a Bevy-ECS physics plugin and cannot run inside an egui application, so the behaviour here is an independent kinematic reimplementation driven by that project's own tuning constants, transcribed from `src/game/config.rs` with their original names and cited source. Trajectories are close to the game's, not bit-identical; the constants are exact.

## Entering and leaving

A `Play` menu carries a `Play / Stop` item, and `F5` is its shortcut; either toggles play mode. Entering spawns the character; leaving removes it and returns the editor to the tool, selection and input handling it had before. `R` restarts the run: the character returns to the spawn point and, per the `play-combat` capability, every orc and coin is restored. Toggles are ignored while a text field wants keyboard input.

The spawn point is the last canvas click position in world coordinates, or (100, 100) when nothing has been clicked. A left click during play moves the spawn point and respawns there rather than acting on the level.

## Non-destructive guarantee

Play mode never modifies the level. While playing, entity placement, selection, dragging, deletion and polygon vertex editing are all disabled; no code path calls `save_state()`; the entity list is unchanged across a play session; and `has_unsaved_changes()` is unaffected by entering, playing or leaving. Play state is never serialised into the level file.

## Collision geometry

Collision is taken from the level's polygon entities, mirroring `world_objects::spawn_polygon_collider` in the game: every polygon entity is solid, and a polygon whose type is `wall_tool` is additionally climbable. The game builds each polygon as a `Collider::convex_hull`, so a concave polygon drawn in the editor collides as its convex hull here too. Bitmap entities are not solid. They are not inert either: the `play-combat` capability turns `orc_tool`, `coin_tool` and `death_trap_tool` into orcs, coins and death pits. Every other bitmap stays decorative.

The character is a capsule with `PLAYER_CAPSULE_HALF_HEIGHT` 20 and `PLAYER_CAPSULE_RADIUS` 25, drawn as a plain shape with no sprite or animation. It never tunnels through a polygon, including at maximum fall speed.

## Coordinate space

The simulation works directly in the editor's world space: top-left origin, Y increasing downward. Gravity is +Y and jumping is −Y. No Y-flip is introduced; the flip belongs to the game's importer.

## Movement

All values below are the game's constants.

| Ability | Behaviour |
| --- | --- |
| Run | Horizontal speed saturates at `PLAYER_RUN_SPEED` 600 px/s, accelerating at `PLAYER_GROUND_ACCEL` 4500 px/s² on the ground and `PLAYER_AIR_ACCEL` 3200 px/s² in the air. |
| Jump | A full-hold jump reaches `JUMP_HEIGHT` 365 px above the standing height. Releasing while rising applies `JUMP_SHORTEN_EXTRA_GRAVITY` for a lower hop; after the apex `JUMP_FALL_EXTRA_GRAVITY` speeds the descent. `MAX_AIR_JUMPS` is 0. |
| Coyote and buffer | A jump is allowed up to `COYOTE_TIME` 0.12 s after leaving the ground, and a press up to `JUMP_BUFFER_TIME` 0.1 s before landing fires on landing. |
| Gravity and fall | Base `GRAVITY` 1400 px/s², plus `PLAYER_FREE_FALL_EXTRA_GRAVITY` while free-falling. Holding down multiplies gravity by `FAST_FALL_GRAVITY_MULT` 1.8. Downward speed never exceeds `MAX_FALL_SPEED` 950 px/s. |
| Dash | `DASH_SPEED` 1500 px/s covering `DASH_DISTANCE` 450 px, then braking to `DASH_BRAKE_TO_SPEED`. `DASH_COOLDOWN` 0.4 s between dashes, `MAX_AIR_DASHES` 1 per airtime reset by landing, and a press is remembered for `DASH_BUFFER_TIME` 0.1 s. |
| Wall slide | Against a wall, descent is capped at `WALL_SLIDE_MAX_FALL_SPEED` 110 px/s. |
| Wall jump | Rises `WALL_JUMP_HEIGHT` 190 px and carries `WALL_JUMP_HORIZONTAL_DISTANCE` 170 px away from the wall; horizontal input is ignored for `WALL_JUMP_INPUT_LOCK` 0.18 s, and the jump is still allowed `WALL_COYOTE_TIME` 0.1 s after leaving the wall. |
| Climb | On a `wall_tool` polygon only, the character clings and moves up or down at `CLIMB_SPEED` 140 px/s, drifting into the wall at `CLIMB_WALL_DRIFT` 60 px/s. A non-`wall_tool` polygon cannot be climbed. |

## Input

Bindings match the game's `PlayerBindings` defaults: move left `A` or `←`, move right `D` or `→`, look up `W` or `↑`, look down `S` or `↓`, jump `Space` or `Z`, dash `Shift`, `K` or `C`, attack `J` or `X`, shield `L`. `F2` toggles the combat debug view. Attack and shield belong to the `play-combat` capability. While play mode is active the arrow keys drive the character and do not scroll the canvas; on Stop they scroll the canvas again.

## Camera

While playing, the canvas scroll follows the character using the game's camera constants — `CAMERA_FOLLOW_RATE_X` 7 and `CAMERA_FOLLOW_RATE_Y` 5 as per-second exponential smoothing rates, a `CAMERA_DEAD_ZONE_X` of 40 px, `CAMERA_OFFSET_Y` 80, and a `CAMERA_FALL_CATCHUP_MULT` of 2.5 once descent exceeds `CAMERA_FALL_SPEED_THRESHOLD` 400 px/s. The character stays on screen while running.

## Timing

The simulation advances on a fixed timestep of 1/120 s driven by an accumulator, so the same inputs produce the same trajectory regardless of display refresh rate. The editor repaints every frame while play mode is active.

## Presentation

A small overlay shows the character's world position, whether it is grounded, and the current state — run, air, dash, wall slide or climb — followed by the health, score, coins, orcs and shield readings the `play-combat` capability adds.

## Structure

The simulation is a pure module with no egui `Context` or `Ui` dependency, advanced by an explicit `dt`, so constants and trajectories are unit-testable headlessly. `main.rs` owns the play-mode flag, spawn point, input capture, drawing and camera.

## Acceptance

- A8 — `Play → Play / Stop` and `F5` each toggle play mode.
- A9 — Spawn is the last click or (100, 100); `R` respawns; a click during play moves the spawn.
- A10 — Play never modifies the level and never affects `has_unsaved_changes()`.
- A11 — Stop restores the previous tool and normal editor input.
- A12 — Polygons are solid; the character lands on them and never tunnels.
- A13 — Run saturates at 600 px/s, reached in about 0.13 s from rest.
- A14 — A full-hold jump reaches about 365 px; an early release is measurably lower.
- A15 — Coyote time 0.12 s and jump buffer 0.1 s both work at their boundaries.
- A16 — Terminal velocity 950 px/s; holding down multiplies gravity by 1.8.
- A17 — Dash covers about 450 px at up to 1500 px/s, 0.4 s cooldown, exactly one air dash.
- A18 — Wall slide caps descent at 110 px/s; wall jump gives about 190 px up and 170 px across with a 0.18 s input lock.
- A19 — Climb works at 140 px/s on a `wall_tool` polygon and not on a plain polygon.
- A20 — Identical trajectories at 60 Hz and 144 Hz.
- A21 — The camera follows the character while playing.
- A22 — Arrow keys drive the character while playing and scroll the canvas when stopped.
