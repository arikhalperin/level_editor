# Outcome

Two fixes to play mode's camera:

1. **The character can no longer leave the screen.** However fast it moves — running, dashing, falling, or being knocked back — the camera keeps it fully inside the canvas with a margin, including when it goes past the level origin into negative world space.
2. **Stopping play puts the view back.** The scroll position the editor had before `F5` is restored exactly when play ends, so testing a level no longer leaves the canvas parked wherever the character happened to die.

# Scope

## Keeping the character on screen

- After the existing smooth follow, the scroll offset is hard-clamped so the character's capsule is fully inside the canvas panel with a `CAMERA_SAFE_MARGIN` of 24 px on every side. The clamp is a bound, not a spring: it holds on the frame it is needed, so no speed and no teleport can put the character outside.
- Inside that safe area the camera behaves exactly as it does now, with the game's constants: `CAMERA_DEAD_ZONE_X` 40, `CAMERA_FOLLOW_RATE_X` 7, `CAMERA_FOLLOW_RATE_Y` 5, `CAMERA_OFFSET_Y` 80, and `CAMERA_FALL_CATCHUP_MULT` 2.5 once descent passes `CAMERA_FALL_SPEED_THRESHOLD` 400.
- The camera centres on the **canvas panel**, not the window. Today it computes its target from `viewport.width()/height()` as though the panel started at (0, 0), but the panel sits below the menu bar, so the character is currently biased downward by the menu-bar height. Using the panel's centre removes that bias and is what makes the margin honest.
- Only the character is kept on screen. The floating toolbox and the play overlay may still overlap it; excluding them was considered and rejected as it would shrink the usable area and shove the camera whenever the toolbox is dragged.

## Showing past the level origin during play

- The editor's scroll offset is clamped to ≥ 0 by `ScrollModel`, and that clamp is reapplied every frame because `ScrollModel::step` runs after the camera. While play mode is active the editor's scroll model is therefore not stepped at all: the camera owns the offset outright and may take it negative, so a character at negative world coordinates is still shown.
- With play stopped, editor scrolling is unchanged and still cannot pass the origin.
- This amends the archived `canvas-scrolling` capability, whose invariant reads "`scroll_offset.x ≥ 0` and `scroll_offset.y ≥ 0` at all times". It becomes "at all times while editing"; play mode's camera is the stated exception.

## Restoring the view

- Entering play captures the editor's whole scroll state (offset and momentum). Stopping play restores that offset exactly and clears momentum, so the canvas neither drifts on nor keeps gliding.
- Restoring happens on every route out of play — `F5` and `Play → Stop` both go through `stop_play`.
- Restarting the run (`R`), dying, and moving the spawn with a click all leave the camera where it is: only stopping play restores the view.
- Entering play again captures the view afresh.

## Structure

The camera becomes a pure module with no egui `Context`/`Ui`, taking the offset, the character's position and velocity, the panel rectangle and `dt`, and returning the new offset — so containment can be unit-tested headlessly at any speed. Capturing and restoring the view are pure helpers on `ScrollModel` in the same module.

# Non-goals

- Changing how the camera feels inside the safe area: the smoothing constants, dead zone and fall catch-up are untouched.
- Keeping the character clear of the toolbox or the overlay.
- Any camera behaviour outside play mode; editor scrolling, its momentum and its origin clamp are unchanged.
- Restoring anything other than the scroll position on stop — the tool and tool name are already restored, and nothing else about the editor is touched by play.
- Zoom, letterboxing, or bounding the camera to the level size.
- Editing `/Users/arikha/bevy_prince_platformer`, whose own camera is a separate system.

# Acceptance examples

- A1 — Never off screen: with the character running at full speed, dashing at 1500 px/s, falling at terminal velocity and being knocked back, its capsule stays fully inside the canvas panel with at least the 24 px margin on every side, at every frame.
- A2 — Hard bound: teleporting the character an arbitrary distance in one frame still leaves it inside the safe area on that same frame — the clamp does not need several frames to catch up.
- A3 — Feel preserved inside the safe area: while the character is well within the viewport the camera still ignores horizontal movement under the 40 px dead zone and converges at the game's rates, with the Y rate multiplied by 2.5 once descent passes 400 px/s.
- A4 — Centres on the panel: given a panel that starts below a menu bar, the camera's resting target puts the character at the panel's centre (offset by `CAMERA_OFFSET_Y`), not the window's.
- A5 — Past the origin: with the character at negative world coordinates the camera follows into negative scroll offset and the character remains on screen.
- A6 — Editing unchanged: with play stopped, the editor's scroll offset still cannot go below zero on either axis.
- A7 — View restored: scroll somewhere, press `F5`, let the character run far away, press `F5` again — the canvas is back at exactly the offset it had before play started.
- A8 — No leftover momentum: the restored view is stationary even if the canvas was mid-glide when play began.
- A9 — Either route: stopping from the `Play` menu restores the view exactly as `F5` does.
- A10 — Only stopping restores: pressing `R`, dying, or clicking to move the spawn leaves the camera following the character; none of them restores the pre-play view.
- A11 — Re-entry re-captures: after stopping, scrolling elsewhere and entering play again, stopping restores the newer position rather than the first one.
- A12 — Still non-destructive: a play session that moves the camera far from the origin leaves the level's entities and `has_unsaved_changes()` untouched.

# Constraints and invariants

- The camera module is pure — no egui `Context` or `Ui`, advanced by an explicit `dt` — so every containment claim is unit-testable headlessly.
- The character's on-screen position is `world − scroll_offset`; the safe area is the canvas panel rectangle inset by `CAMERA_SAFE_MARGIN` and by the capsule's half extents (`PLAYER_CAPSULE_RADIUS` 25 wide, `PLAYER_CAPSULE_HALF_EXTENT_Y` 45 tall).
- If the panel is ever smaller than the character plus its margins, the character is centred rather than clamped to an empty range.
- `ScrollModel`'s own origin clamp is unchanged; it is simply not stepped while play mode owns the camera.
- No new dependencies; no change to the level format, the simulation, or combat.
- `cargo build` and `cargo test` keep passing; the existing 115 tests stay green.

# Decisions

- D1 — Workspace: current directory. The tree is clean.
- D2 — During play the camera may show past the level origin so the character is always visible; editing keeps its origin clamp (user choice). The archived `canvas-scrolling` invariant is amended accordingly.
- D3 — The safe area is the canvas panel inset by a margin; the toolbox and overlay are not avoided (user choice).
- D4 — (Agent) `CAMERA_SAFE_MARGIN` is 24 px: enough that the capsule never touches an edge, small enough not to feel like a shrunken viewport. It is an editor constant, not a game one, and is named as such.
- D5 — (Agent) The clamp is applied after the smoothing rather than by raising the follow rate, because only a bound gives the guarantee the user asked for; a faster spring merely makes leaving the screen less likely.
- D6 — (Agent) While playing, the editor's `ScrollModel` is not stepped at all rather than being given a "may go negative" mode. That keeps the model's own invariant honest and leaves editor scrolling untouched.
- D7 — (Agent) Correcting the camera's centre from the window to the panel is in scope: it is a pre-existing bias that directly undermines the containment guarantee being added.

# Open questions

None. Shared understanding (scope, D1-D7, A1-A12, non-goals, current-directory workspace) was explicitly confirmed by the user on 2026-09-12.

# Verification expectations

- `cargo build` succeeds with no new warnings; `cargo test` passes.
- Unit tests on the pure camera module, all headless: the character stays inside the inset panel across a long run at full speed, a dash, a terminal-velocity fall and a large single-frame teleport (A1, A2); the dead zone holds the camera still for sub-40 px movement and the rates match the game's exponential form, with the fall multiplier applied past 400 px/s (A3); the resting target centres the character on a panel whose origin is not (0, 0) (A4); a character at negative world coordinates yields a negative offset and stays on screen (A5); a panel narrower than the character plus margins centres rather than producing an inverted clamp.
- Unit tests on the capture/restore helpers: the offset comes back exactly and momentum is cleared (A7, A8).
- Verifier code inspection in `main.rs`: the camera is called with the panel rect; `ScrollModel::step` is skipped only while playing, leaving the editor clamp intact (A6); `stop_play` restores and `start_play` captures, on both the `F5` and menu routes (A9, A11); `R`, death and spawn clicks do not restore (A10); no entity-mutating path is touched (A12).
- Smoke run: the app launches, stays alive and does not panic.
