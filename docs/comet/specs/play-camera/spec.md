# Play camera

Complete target behaviour of the camera that follows the character in play mode. It extends the `play-simulation` capability, which owns the character, the spawn point and the fixed timestep, and amends the `canvas-scrolling` capability's origin invariant as noted below.

## Coordinates

The character's on-screen position is `world − scroll_offset`. The camera's job is to choose `scroll_offset` each frame so that position falls inside the canvas panel.

## Following

Each frame the camera aims to put the character at the **canvas panel's** centre, offset by `CAMERA_OFFSET_Y` 80 so the character sits below it, and converges on that target with per-second exponential smoothing, `k = 1 − exp(−rate · dt)`:

| Axis | Behaviour |
| --- | --- |
| X | Ignored entirely while the target is within `CAMERA_DEAD_ZONE_X` 40 of the current offset; otherwise converges at `CAMERA_FOLLOW_RATE_X` 7. |
| Y | Converges at `CAMERA_FOLLOW_RATE_Y` 5, multiplied by `CAMERA_FALL_CATCHUP_MULT` 2.5 once the character descends faster than `CAMERA_FALL_SPEED_THRESHOLD` 400 px/s. |

The centre is taken from the panel rectangle, not from the window: the panel sits below the menu bar, so centring on the window would bias the character downward by the menu bar's height.

## Containment

After the smoothing, the offset is clamped so the character's capsule — `PLAYER_CAPSULE_RADIUS` 25 either side, `PLAYER_CAPSULE_HALF_EXTENT_Y` 45 above and below — lies wholly inside the panel inset by `CAMERA_SAFE_MARGIN` 24 px on every side.

Writing the inset panel as `safe`, the offset is bounded by

```
offset.x ∈ [ pos.x − safe.max.x + half_w ,  pos.x − safe.min.x − half_w ]
offset.y ∈ [ pos.y − safe.max.y + half_h ,  pos.y − safe.min.y − half_h ]
```

This is a bound rather than a spring, so it holds on the frame it is needed: no speed, and no discontinuous jump in the character's position, can put the character outside the safe area even for one frame. Inside the safe area the clamp never binds and the smoothing above is the only thing acting.

If the panel is smaller than the character plus its margins on an axis, the bound inverts; the character is then centred on that axis instead.

Only the character is contained. The floating toolbox and the play overlay may overlap it.

## Past the level origin

While play mode is running the camera owns the scroll offset outright and may take it **negative**, so a character at negative world coordinates is still shown. The editor's `ScrollModel` is not stepped during play, which is what allows this: the model's own origin clamp is unchanged and would otherwise reapply every frame.

This amends the `canvas-scrolling` capability, which states that `scroll_offset.x ≥ 0` and `scroll_offset.y ≥ 0` at all times. That invariant holds **while editing**; the play camera is its one exception. With play stopped, editor scrolling still cannot pass the origin in either axis.

## Remembering the editor's view

Entering play captures the editor's scroll state — offset and momentum. Stopping play restores that offset exactly and clears momentum, so the canvas returns to where the user left it and does not drift on.

Restoring happens on every route out of play: the `F5` shortcut and `Play → Stop` both go through the same path. Restarting the run with `R`, dying, and clicking to move the spawn all leave the camera following the character; only stopping restores. Entering play again captures the view afresh, so the restored position is always the one from the most recent entry.

## Structure

The camera is a pure module with no egui `Context` or `Ui`, taking the current offset, the character's position and velocity, the panel rectangle and `dt`, and returning the new offset, so containment is unit-testable headlessly at any speed. Capturing and restoring the editor's view are pure helpers over `ScrollModel` in the same module. `main.rs` supplies the panel rectangle and owns when play starts and stops.

## Acceptance

- A1 — The character's capsule stays fully inside the inset panel at run, dash, terminal-fall and knockback speeds.
- A2 — A single-frame teleport of any distance still leaves the character inside the safe area that frame.
- A3 — Inside the safe area the dead zone and the game's smoothing rates, including the fall catch-up, are unchanged.
- A4 — The resting target centres the character on the panel, not the window.
- A5 — A character at negative world coordinates is followed into negative scroll offset and stays on screen.
- A6 — With play stopped, editor scrolling still cannot pass the origin.
- A7 — Stopping play restores the exact pre-play scroll offset.
- A8 — The restored view has no momentum.
- A9 — Both the `F5` and menu routes restore.
- A10 — `R`, death and spawn clicks do not restore.
- A11 — Re-entering play captures the view again.
- A12 — Play remains non-destructive.
