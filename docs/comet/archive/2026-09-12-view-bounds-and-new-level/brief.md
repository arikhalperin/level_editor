# Outcome

Two editor changes:

1. **Scrolling is bounded.** The canvas can no longer be scrolled off into endless empty space. The view is clamped to the level plus one screen of slack, so there is always a screenful of room past the boundary to place entities in and grow the level, but no way to lose the level entirely.
2. **`File → New Level`.** Start a fresh, empty level from inside the editor, with a prompt to save first when there is unsaved work.

# Scope

## Bounded scrolling

- The editor's scroll offset is clamped to `0 ≤ offset ≤ level_size` on each axis, where `level_size` is the resolved level extent (explicit size, else background, else entity bounds, else 1920×1080).
- That upper bound is exactly "one screen of slack": since the visible region is `offset … offset + viewport`, an offset of `level_size` puts the level's far edge at the viewport's near edge, leaving a full viewport of empty space beyond the boundary. Entities can still be placed there, so a level can still be grown by working past its edge and enlarging it afterwards — the behaviour the `level-size` capability depends on.
- Momentum stops at the upper bound exactly as it already does at the origin: the offset is pinned and that axis's velocity is zeroed, with no overshoot and no bounce.
- The bound tracks the level size live: enlarging the level through `Level → Level Size…` immediately allows scrolling further, and shrinking it pulls the reachable area in.
- A level smaller than the viewport still scrolls, over the range `0 … level_size`; the bound can never invert, because `level_size` is always at least 1 px.
- **Play mode is exempt.** Its camera owns the offset and may take it outside this range — including negative — to keep the character on screen, exactly as the `play-camera` capability requires. Stopping play restores the pre-play editor offset, which is inside the bound by construction.
- This amends the archived `canvas-scrolling` capability, which states there is "**no** upper bound on either axis", and the `level-size` capability's "scrolling and entity placement remain unbounded beyond the boundary". Placement stays unbounded within the reachable area; the view no longer is.

## New Level

- A `New Level` item at the top of the `File` menu, above `Load Level`, and disabled while play mode is running for the same reason `Load Level` is.
- When there are unsaved changes it opens the same confirmation the editor already uses for `Exit`, offering **Save**, **Discard** and **Cancel**. Cancel abandons the operation and changes nothing. With no unsaved changes it proceeds straight away.
- The existing `Exit` confirmation is generalised so both commands share one dialog and one code path rather than duplicating it. Choosing **Save** goes ahead only when the level was actually written; dismissing the destination dialog leaves the confirmation open rather than discarding the work.
- Starting a new level clears, in the editor's memory: all entities, the explicit level size, the background image and its size, the play spawn point, the current selection and any polygon-edit state, and the undo and redo history. The view returns to the origin.
- The remembered level path is cleared, so the next `Save Level` asks where to put the file rather than silently overwriting the level that was open.
- Afterwards the editor reports no unsaved changes, so quitting immediately does not prompt.
- New Level never touches the disk: no file is written, deleted or overwritten.

# Non-goals

- Any change to entity **placement** bounds: entities may still be created anywhere in the reachable area, including past the level boundary.
- Changing play mode's camera, which keeps its own rules and its exemption from the editor bound.
- A template or starter level: the new level is empty, not pre-populated.
- Multiple documents, tabs, or a recent-files list.
- Rewriting the on-disk config. `New Level` clears the session's background and level; the config still remembers the last files opened, so restarting the editor reloads them. Nothing was saved, so nothing is lost — but a restart is not a way to keep a new level.
- Zoom, or fitting the level to the window.

# Acceptance examples

- A1 — Clamped: scrolling right or down stops with the offset at the resolved level size on that axis; scrolling left or up still stops at 0.
- A2 — A screen of slack: at the far bound the level's edge sits at the near edge of the viewport, leaving a full viewport of empty space visible past the boundary.
- A3 — Momentum stops cleanly: releasing an arrow key while gliding toward the far bound pins the offset there with zero velocity, no overshoot and no bounce, mirroring the origin.
- A4 — Follows the level size: enlarging the level via `Level → Level Size…` immediately permits scrolling further, and shrinking it reduces the reachable area.
- A5 — Placement still works past the boundary: an entity can be created in the slack area beyond the level edge, and the level can then be enlarged to include it.
- A6 — Play is exempt: during play the camera may put the offset outside `0 … level_size` to keep the character on screen, and stopping play restores an offset inside the bound.
- A7 — Small levels: with a level smaller than the viewport the offset range is `0 … level_size` and never inverts.
- A8 — `File → New Level` exists above `Load Level` and is disabled while playing, with the same hover explanation as `Load Level`.
- A9 — Prompted when dirty: with unsaved changes, `New Level` opens the confirmation offering Save, Discard and Cancel; Cancel leaves every entity, the level size and the background untouched.
- A10 — Immediate when clean: with no unsaved changes, `New Level` starts the new level without a prompt.
- A11 — Cleared: after `New Level` there are no entities, no explicit level size, no background image, no spawn point, no selection or polygon-edit state, and the undo and redo histories are empty.
- A12 — View reset: after `New Level` the canvas is back at the origin.
- A13 — Reads as clean: immediately after `New Level` the editor reports no unsaved changes, so `Exit` does not prompt.
- A14 — Path cleared: after `New Level`, `Save Level` asks for a destination rather than reusing the previously opened level's path.
- A15 — No disk writes: `New Level` writes, deletes and overwrites nothing.

# Constraints and invariants

- The scroll bound lives in `ScrollModel` alongside the existing origin clamp, so the pure model owns both ends of the range and both are unit-testable headlessly; the caller supplies the maximum each step.
- `0 ≤ offset ≤ max` holds after every editor step, and `max ≥ 0` always, so the range can never invert.
- Play mode does not step `ScrollModel` at all, which is what keeps its camera exempt; that arrangement is unchanged.
- New Level is a pure in-memory reset plus a dialog; it performs no file I/O.
- The level format, the simulation, combat and the play camera are untouched.
- No new dependencies. `cargo build` and `cargo test` keep passing; the existing 136 tests stay green.

# Decisions

- D1 — Workspace: current directory. The tree is clean.
- D2 — The view is clamped to the level plus one screen of slack, rather than strictly to the level, so entities can still be placed past the boundary and a level can still be grown that way (user choice).
- D3 — `New Level` clears everything including the background image (user choice).
- D4 — (Agent) "One screen of slack" is expressed as `offset ≤ level_size`, which is exactly one viewport of space past the far edge whatever the window size, and needs no separate constant.
- D5 — (Agent) The bound goes in `ScrollModel::step` rather than being applied afterwards in `main.rs`, so momentum stops at the far edge the same way it stops at the origin instead of being clipped after the fact.
- D6 — (Agent) `New Level` prompts through the existing unsaved-changes dialog, generalised to carry which action is pending, rather than adding a second near-identical dialog.
- D7 — (Agent) The remembered level path is cleared so `Save Level` cannot silently overwrite the previously open level with the new empty one.
- D8 — (Agent) Choosing **Save** in the confirmation goes ahead only when the level was actually written; a dismissed destination dialog keeps the confirmation open rather than throwing the work away.

# Open questions

None. Shared understanding (scope, D1-D8, A1-A15, non-goals, current-directory workspace) was explicitly confirmed by the user on 2026-09-12.

# Verification expectations

- `cargo build` succeeds with no new warnings; `cargo test` passes.
- Unit tests on `ScrollModel`: the offset never exceeds the supplied maximum on either axis however long an arrow key is held (A1); momentum arriving at the far bound stops there with zero velocity, no overshoot and no bounce, matching the existing origin test (A3); a maximum that changes between steps takes effect immediately, both enlarging and shrinking the reachable area, with an offset already beyond a shrunk maximum pulled back to it (A4); a maximum of zero or a level smaller than the viewport still gives a valid, non-inverting range (A7); and the existing origin-clamp tests still pass unchanged.
- Unit tests on the slack rule: the maximum offset derived from a level size and viewport leaves exactly one viewport of space past the level's far edge (A2).
- Unit tests on the new-level reset, as a pure operation over the editor's level state: everything listed in A11 is cleared, the view is at the origin (A12) and the resulting state hashes as clean (A13).
- Verifier code inspection in `main.rs`: the `New Level` item's position, its disabled state during play and its hover text (A8); the shared dialog offering Save / Discard / Cancel with Cancel a no-op (A9) and the no-prompt path when clean (A10); the cleared level path (A14); the absence of any file operation on the New Level path (A15); the bound being supplied from the resolved level size and not applied during play (A5, A6).
- Smoke run: the app launches, stays alive and does not panic.
