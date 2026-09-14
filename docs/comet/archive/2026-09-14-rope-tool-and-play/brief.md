# Outcome

Levels can contain ropes. The editor gets a rope tool that hangs a rope from an anchor and saves it in the level file, and the editor's play mode lets the character grab the rope, swing on it, climb along it, and let go with momentum. The swing model and its constants are written down so the Bevy game (`~/bevy_prince_platformer`) can reproduce them in a follow-up change of its own.

# Scope

- A `rope_tool` in the toolbox (new tool kind `rope`, with an icon). Placement is two clicks: the top anchor, then the bottom end. The rope is vertical at rest.
- A rope entity on the canvas: drawn as a line from the anchor, selectable, draggable, deletable, undoable like other entities.
- Level JSON: a rope is saved as a `bitmap` entry named `rope_tool` whose `position` is the anchor and whose `size` is `[thickness, length]`, so the schema is unchanged and older consumers see an ordinary bitmap.
- Play mode: grabbing (hold up or down while overlapping the rope), a damped pendulum swing pumped by left / right, climbing along the rope with up / down, release with the jump button carrying the swing velocity plus a hop, and the rope continuing to swing after release.
- The shared rope constants live in `game_config.rs` next to the other transcribed tuning, documented as the model the game change must match.
- Help window rows, README, `LEVEL_FORMAT.md` and `BEVY_INTEGRATION.md` mention ropes.

Single Native change: the tool, the entity, the simulation and the help all touch `main.rs` / `sim.rs` and are verified by the same play session, so decomposition would cost more than it returns.

# Non-goals

- The game side (importer and swinging in `~/bevy_prince_platformer`); it is a separate Native change in that repository, in a worktree, after this one.
- Rope art, sound or animation beyond a plain drawn line; no sprite.
- Angled or slack ropes, rope-to-rope jumps, rope cutting, or ropes attached to moving things.
- Changing an existing rope's length after placement (delete and redraw instead).
- Changing the level JSON schema or the game's coordinate transform.
- Retuning any existing movement constant.

# Acceptance examples

- A1 — With the Rope tool active, a first click at world (400, 100) and a second click at (430, 400) creates one rope anchored at (400, 100) with length 300. A second click above the anchor or less than 60 px below it does nothing; Escape (or switching tools) cancels the rope in progress; a preview line follows the cursor between the two clicks.
- A2 — A rope is drawn as a vertical line from its anchor with a small anchor knob. With the Select tool, clicking within 8 px of the line selects it and dragging moves the whole rope; the Delete tool removes it; undo and redo apply to placing, moving and deleting a rope.
- A3 — Saving writes the rope as `{"type":"bitmap","position":[400,100],"bitmap_name":"rope_tool","size":[6,300]}`; loading that file restores a rope with the same anchor and length; a level file with no ropes loads exactly as before.
- A4 — In play, holding up or down while the character's centre is within 30 px of the rope's line (and at least 40 px below the anchor) attaches the character to the rope at that height; the overlay state reads `rope`; the character hangs at the hold point with zero velocity if it arrived slowly.
- A5 — Released from rest at 30° with no input, the character swings through the bottom and back to within 10% of a period of `2π·sqrt(L / GRAVITY)`, and the damped swing settles to hanging straight down.
- A6 — From a hanging rest, holding right builds a swing; the rope's angle never exceeds 75° from vertical however long the key is held.
- A7 — Up / down move the hold point along the rope at `CLIMB_SPEED` 140 px/s, no closer than 40 px to the anchor and no further than the rope's length.
- A8 — Pressing jump while on the rope releases the character with the swing's tangential velocity plus an upward speed equal to a `WALL_JUMP_HEIGHT` 190 px jump; the rope cannot be re-grabbed for 0.25 s; landing on ground afterwards behaves as any landing.
- A9 — Jumping into a rope at run speed starts a swing whose initial angular velocity equals the character's velocity projected on the rope's tangent divided by the hold length.
- A10 — Hitting a solid polygon while swinging pushes the character out as usual and stops the swing; being hurt or dying drops the rope; dash and wall jump are unavailable while on a rope.
- A11 — After release the rope keeps swinging with damping until it hangs still; a grab takes the rope's current angle; while editing (not playing) every rope hangs straight down.
- A12 — Play never modifies a rope entity and never affects `has_unsaved_changes()`.
- A13 — The help window's Tools section lists `Rope` with the kind hint `click the anchor, then click the bottom end`; its Mouse section has a Rope-tool row; the up/down, left/right, jump and Escape rows mention ropes; the README, `LEVEL_FORMAT.md` and `BEVY_INTEGRATION.md` describe the rope entry.
- A14 — `game_config.rs` carries the rope constants (`ROPE_BITMAP`, `ROPE_THICKNESS`, `ROPE_MIN_LENGTH`, `ROPE_MIN_HOLD`, `ROPE_GRAB_HALF_WIDTH`, `ROPE_PUMP_ACCEL`, `ROPE_DAMPING`, `ROPE_MAX_ANGLE_DEG`, `ROPE_RELEASE_HOP_HEIGHT`, `ROPE_REGRAB_LOCK`) with the equations in the `rope-swing` spec as the model for the game.
- A15 — `cargo test` and `cargo build` pass, with unit tests for placement, serialisation round trip, and the pendulum (period, angle cap, pump, climb, release, regrab lock).

# Constraints and invariants

- Editor entities today are bitmaps (`position`, `bitmap_name`, `size` from the icon image) or polygons; tool kinds are `bitmap`, `polygon`, `tool`. A rope needs its own entity variant in the editor because its size is its length, not an icon's size; it still serialises as a bitmap entry.
- The game's checkpoint primitive reused the bitmap form (`checkpoint_tool`) for the same reason; the game importer branches on `bitmap_name`.
- Both the game and the editor simulation already implement cling + climb at `CLIMB_SPEED` on `wall_tool` polygons; the rope reuses the same input rule (hold up / down) and climb speed.
- The simulation stays a pure module advanced with an explicit `dt` on the fixed 1/120 s step; the pendulum is integrated there and is deterministic.
- Editor world space is Y-down: the rope hangs toward +Y; the game importer flips Y as it does for every entity.
- The game repository has its own Comet Native project with an active change `level1-moveset-challenges` in Build and uncommitted work in its working directory; rope work there is a separate change in a worktree.

# Decisions

- D1 (Q1) — The rope is a **swinging pendulum**. Confirmed 2026-09-14.
- D2 (Q2) — Placement is **click the top anchor, then click the bottom end**; vertical at rest. Confirmed 2026-09-14.
- D3 (Q3) — Grab by **holding up or down while overlapping** the rope; no automatic grab. Confirmed 2026-09-14.
- D4 (Q4) — **Editor first, then the game** as a separate Native change in the game repository, in a worktree. Confirmed 2026-09-14.
- D5 (Q5) — While on the rope, **up / down climb along it** at `CLIMB_SPEED`, changing the swing radius. Confirmed 2026-09-14.
- D6 (Q6) — **Left / right pump** the swing with a tangential acceleration, capped at 75° from vertical. Confirmed 2026-09-14.
- D7 (Q7) — Jump releases with the **swing velocity plus a 190 px hop** (`WALL_JUMP_HEIGHT`). Confirmed 2026-09-14.
- D8 — Level representation: a `bitmap` entry named `rope_tool`, `position` = anchor, `size` = `[ROPE_THICKNESS 6, length]`; schema unchanged. Chosen over a new entity type so every consumer keeps loading the file.
- D9 — One pendulum model with named constants: `ROPE_PUMP_ACCEL` 900 px/s², `ROPE_DAMPING` 0.35 /s, `ROPE_MAX_ANGLE_DEG` 75, `ROPE_GRAB_HALF_WIDTH` 30 px, `ROPE_MIN_HOLD` 40 px, `ROPE_MIN_LENGTH` 60 px, `ROPE_REGRAB_LOCK` 0.25 s, gravity `GRAVITY` 1400. The rope keeps its own angle and angular velocity and goes on swinging (damped) after release.
- D10 — While on a rope: dash and wall jump are unavailable; attack and shield behave as in `play-combat`; a non-normal condition (hurt, dead) drops the rope. Solid geometry stops the swing.
- D11 — Rope length is fixed at placement; there is no length handle (non-goal).

# Open questions

None. Shared understanding confirmed by the user on 2026-09-14.

# Verification expectations

- `cargo test` (pure tests for the rope entity, serialisation, tool state machine and pendulum) and `cargo build` warning-free for touched code.
- A manual play session on a level with a rope over a gap, if a GUI session is available.
