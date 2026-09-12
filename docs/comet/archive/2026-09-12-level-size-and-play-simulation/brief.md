# Outcome

Two additions to the level editor:

1. **Explicit level size.** A `Level → Level Size…` dialog sets the level's width and height in pixels. The size is drawn as a boundary on the canvas, drives the minimap extent, and is saved to the level JSON as a new `level_size` field.
2. **Play simulation.** A Play/Stop toggle drops a controllable character into the level being edited, so level geometry can be tested without leaving the editor. Movement reproduces `/Users/arikha/bevy_prince_platformer` — run, variable-height jump, coyote time and jump buffering, fast fall, dash, wall slide, wall jump, and wall climbing — using that project's exact tuning constants from `src/game/config.rs`.

# Scope

## Level size

- `Level` menu with a `Level Size…` item opening a modal dialog with numeric Width and Height fields, plus OK and Cancel. Cancel leaves the size unchanged.
- Initial dialog values come from the current level size: the saved `level_size` if present, else the background image size, else the bounding box of all entities, else 1920×1080.
- Width and height are clamped to a sane range (1 … 1 000 000 px) and rejected if not finite.
- The level boundary is drawn on the canvas as a thin outline from world (0, 0) to (width, height), so the editable area is visible while scrolling.
- The minimap uses the explicit level size as its extent when one is set.
- `LevelData` gains `level_size: Option<[f32; 2]>`, written on save and read on load. Absent (older files) keeps today's behaviour exactly.
- Scrolling stays unbounded in the positive direction, as confirmed in the two-axis-smooth-scrolling change: the boundary is a marker, not a scroll clamp, so entities can still be placed beyond it and the level grown afterwards.

## Play simulation

- `Play` menu with a `Play / Stop` item, and `F5` as its shortcut. Both toggle play mode.
- Entering play mode spawns the character at the last canvas click position in world coordinates, or at (100, 100) when nothing has been clicked yet. `R` respawns at that point. While in play mode, a left click moves the spawn point and respawns there.
- Play mode is strictly non-destructive: entity placement, selection, dragging, deletion and polygon editing are all disabled while playing, the level is never modified, and `has_unsaved_changes()` is unaffected. `Stop` returns to the editor with the tool, selection and scroll state it had.
- Collision geometry is the level's polygon entities, matching how the game loads them: every polygon is solid; polygons whose type is `wall_tool` are additionally climbable. Bitmap entities are not solid (the game gives them their own behaviour, which is out of scope here).
- The character is a capsule matching the game (`PLAYER_CAPSULE_HALF_HEIGHT` 20, `PLAYER_CAPSULE_RADIUS` 25), drawn as a simple shape — no sprites or animation.
- Movement abilities, all with the game's constants:
  - **Run** — `PLAYER_RUN_SPEED` 600 px/s, `PLAYER_GROUND_ACCEL` 4500, `PLAYER_AIR_ACCEL` 3200 px/s².
  - **Jump** — apex `JUMP_HEIGHT` 365 px, shortened on release (`JUMP_SHORTEN_EXTRA_GRAVITY`), faster descent (`JUMP_FALL_EXTRA_GRAVITY`), `COYOTE_TIME` 0.12 s, `JUMP_BUFFER_TIME` 0.1 s, `MAX_AIR_JUMPS` 0.
  - **Gravity / fall** — `GRAVITY` 1400, `PLAYER_FREE_FALL_EXTRA_GRAVITY`, `FAST_FALL_GRAVITY_MULT` 1.8 while holding down, `MAX_FALL_SPEED` 950.
  - **Dash** — `DASH_SPEED` 1500, `DASH_DISTANCE` 450, `DASH_COOLDOWN` 0.4 s, `MAX_AIR_DASHES` 1, `DASH_BUFFER_TIME` 0.1 s.
  - **Wall slide / wall jump** — `WALL_SLIDE_MAX_FALL_SPEED` 110, `WALL_JUMP_HEIGHT` 190, `WALL_JUMP_HORIZONTAL_DISTANCE` 170, `WALL_JUMP_INPUT_LOCK` 0.18 s, `WALL_COYOTE_TIME` 0.1 s.
  - **Climb** — on `wall_tool` polygons only: `CLIMB_SPEED` 140 px/s up and down, `CLIMB_WALL_DRIFT` 60.
- Key bindings match the game's `PlayerBindings` defaults: move A/D or ←/→, look up/down W/S or ↑/↓, jump Space or Z, dash Shift/K/C. Arrow keys drive the character while playing instead of scrolling the canvas.
- The canvas follows the character while playing, using the game's camera constants (`CAMERA_FOLLOW_RATE_X/Y`, `CAMERA_DEAD_ZONE_X`, `CAMERA_OFFSET_Y`, fall catch-up).
- The simulation runs on a fixed timestep with an accumulator so behaviour does not vary with frame rate, and the editor repaints continuously while play mode is active.
- A small status overlay shows the character's world position, whether it is grounded, and the current state (run / air / dash / wall slide / climb).

# Non-goals

- **Combat, orc AI, coins, death pits, shield and animation** — deferred to a separate follow-up change, as agreed. The simulation has no enemies, no health, no attacks and no collectibles.
- Sprites, sprite animation, VFX (dust, sparks, afterimages), screen shake, squash/stretch and audio.
- Editing `/Users/arikha/bevy_prince_platformer`. Its hard-coded `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0` Y-flip is documented in this brief and in the spec as a required follow-up on the game side; this change does not touch that repository.
- Bit-identical parity with Tnua. Tnua is a physics-engine floating controller on Rapier and cannot run inside an egui app; this is an independent kinematic reimplementation with the same constants. Feel is close, not identical.
- Making level size clamp scrolling, or auto-shrinking the level to fit its content.
- Resizing by dragging a boundary handle on the canvas.
- Rebindable keys or a settings screen.
- Updating `README.md` (already stale from two prior changes).

# Acceptance examples

- A1 — Level Size dialog: `Level → Level Size…` opens a dialog whose Width and Height are pre-filled from the current level size; entering new values and pressing OK changes the level size, and Cancel leaves it unchanged.
- A2 — Size fallback order: with no saved `level_size`, the dialog and the level extent use the background image size; with no background, the bounding box of all entities; with neither, 1920×1080.
- A3 — Persisted: saving writes `level_size` into the level JSON, and loading that file restores the size. A level file without `level_size` loads exactly as before with no error.
- A4 — Boundary drawn: with a level size set, an outline is drawn on the canvas from world (0,0) to (width, height), scrolling with the level like every other element.
- A5 — Minimap extent: with an explicit level size, the minimap's thumbnail area and viewport rectangle are computed against that size rather than the background size.
- A6 — Rejects bad input: width or height that is non-finite, ≤ 0, or above 1 000 000 is refused and the previous size is kept.
- A7 — Scrolling still unbounded: after setting a small level size, the canvas still scrolls past the boundary in both axes and entities can still be placed beyond it.
- A8 — Play toggle: `Play → Play / Stop` and `F5` each toggle play mode; the character appears on entering and disappears on leaving.
- A9 — Spawn point: entering play spawns the character at the last canvas click in world coordinates, or at (100, 100) if nothing was clicked. `R` respawns there. A left click during play moves the spawn and respawns.
- A10 — Non-destructive: during play, clicking does not place, select, delete or move any entity; the entity list is byte-identical before and after a play session, and `has_unsaved_changes()` is unchanged by entering, playing and leaving.
- A11 — Editor restored: leaving play mode restores the previously selected tool and the editor's normal input handling.
- A12 — Solid geometry: the character lands on and is blocked by polygon entities, and does not pass through them when running or falling at full speed.
- A13 — Run constants: horizontal speed saturates at 600 px/s, reaching it in about 0.13 s from rest on the ground.
- A14 — Jump height: a full-hold standing jump rises about 365 px above the standing height, within a 10% tolerance; releasing the jump key early produces a measurably lower hop.
- A15 — Coyote time and jump buffer: a jump pressed up to 0.12 s after walking off a ledge still jumps; a jump pressed up to 0.1 s before landing fires on landing.
- A16 — Fall limits: downward speed never exceeds 950 px/s, and holding down while falling increases the downward acceleration by 1.8×.
- A17 — Dash: a ground dash covers about 450 px at up to 1500 px/s, cannot be repeated within 0.4 s, and exactly one air dash is allowed before touching the ground again.
- A18 — Wall slide and wall jump: sliding down a wall caps descent at 110 px/s; a wall jump rises about 190 px and carries about 170 px away from the wall, with horizontal input ignored for 0.18 s.
- A19 — Climb: against a `wall_tool` polygon the character climbs up and down at 140 px/s; against a non-`wall_tool` polygon it does not climb.
- A20 — Frame-rate independence: stepping the simulation with the same inputs at 60 Hz and at 144 Hz produces the same trajectory within a small tolerance.
- A21 — Camera follows: while playing, the canvas scroll follows the character, and the character stays on screen when running to the right for several seconds.
- A22 — Arrow keys drive the character while playing and do not scroll the canvas; on Stop they scroll the canvas again.

# Constraints and invariants

- The simulation core is a pure module with no egui `Context`/`Ui` dependency, advanced by an explicit `dt`, so the constants and trajectories are unit-testable headlessly.
- Every tuning value is a named constant transcribed from `/Users/arikha/bevy_prince_platformer/src/game/config.rs`, with the game's name preserved and the source file cited, so drift is auditable.
- The editor's world space is top-left origin, Y-down. The simulation works in that space directly: gravity is +Y, jumping is −Y. No Y-flip is introduced in the editor.
- Consequence for the game, documented not implemented: the game converts editor Y with the constant `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0`, so a level whose height is not 720 will be vertically offset when the game loads it. The game must be changed to read the height from the level JSON's `level_size`. This change only writes the field.
- Play mode never mutates `entities`, never calls `save_state()`, and never changes `last_save_hash`.
- Level format stays backward compatible: `level_size` is `Option`, defaulted on deserialise, so existing level files load unchanged.
- No new dependencies; `eframe`/`egui` 0.27 only.
- The `[profile.dev.package.objc2] debug-assertions = false` startup fix stays in place.
- `cargo build` and `cargo test` keep passing; the existing 33 tests stay green.

# Decisions

- D1 — Workspace: current directory. Only an unrelated `README.md` edit is uncommitted.
- D2 — Level size is set through a `Level → Level Size…` dialog and saved to the level JSON as `level_size` (user choice).
- D3 — The editor writes `level_size`; the Bevy game's 720 Y-flip is documented as a follow-up and that repository is not touched (user choice).
- D4 — Abilities in this change: run, jump, dash, wall slide, wall jump, climb. Combat, orcs, coins, shield and animation are a separate follow-up change (user choice, after being shown the ~2,900-line measurement).
- D5 — Play starts from a Play/Stop toggle and spawns at the last click, with Stop restoring the editor (user choice).
- D6 — (Agent) Movement is reimplemented kinematically rather than ported: `bevy-tnua` 0.31 on `bevy_rapier2d` 0.33 is a physics-engine floating character controller and cannot be embedded in an egui app. Only the constants transfer exactly.
- D7 — (Agent) Fixed-timestep accumulator at 1/120 s so trajectories do not depend on the display's frame rate; the game gets the same guarantee from Rapier's fixed step.
- D8 — (Agent) Collision is capsule-vs-convex-polygon resolved iteratively, mirroring the game, which builds a `Collider::convex_hull` per polygon. Concave polygons drawn in the editor therefore collide as their convex hull, exactly as the game treats them.
- D9 — (Agent) Jump initial velocity is derived from the apex constant as `v = sqrt(2 · g · JUMP_HEIGHT)` rather than copying Tnua's jump calculator, which compensates for its own extra-gravity phases. Height is the observable the acceptance item pins.
- D10 — (Agent) All polygons are solid and `wall_tool` polygons are additionally climbable, mirroring `world_objects::spawn_polygon_collider` (`is_wall = polygon_type == "wall_tool"` → `ClimbableWall`, else `Ground`).

# Open questions

None. Shared understanding (scope, D1-D10, A1-A22, non-goals, current-directory workspace, and the staged deferral of combat/orcs/coins/shield) was explicitly confirmed by the user on 2026-09-12.

# Verification expectations

- `cargo build` succeeds with no new warnings in the new modules; `cargo test` passes.
- Unit tests on the pure simulation module, all headless, asserting against the transcribed constants: run speed saturation and time-to-full-speed (A13); jump apex within tolerance and a shortened hop on early release (A14); coyote window and jump buffer at their exact boundaries (A15); terminal velocity and fast-fall multiplier (A16); dash distance, peak speed, cooldown and the single air dash (A17); wall-slide descent cap, wall-jump apex and horizontal carry, and the input lock (A18); climb speed on a `wall_tool` polygon and no climb on a plain polygon (A19); identical trajectories at 60 Hz and 144 Hz (A20); a character dropped onto a polygon lands on its surface and does not tunnel at maximum fall speed (A12).
- Unit tests on the level-size model: fallback order (A2), validation rejecting non-finite, zero, negative and oversized values (A6), and round-tripping `level_size` through `LevelData` serialise/deserialise including a file without the field (A3).
- Verifier code inspection in `main.rs`: the `Level` menu and dialog wiring (A1); the boundary drawing (A4); the minimap extent (A5); scrolling still unclamped (A7); the `Play` menu and `F5` toggle (A8); spawn/respawn handling (A9); every entity-mutating path gated off during play and `has_unsaved_changes` untouched (A10); tool/selection restoration on Stop (A11); camera follow (A21); arrow keys routed to the character only while playing (A22).
- Smoke run: the app launches, stays alive and does not panic.
