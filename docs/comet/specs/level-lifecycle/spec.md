# Level lifecycle and view bounds

Complete target behaviour of two editor concerns after this change is archived: how far the canvas may be scrolled while editing, and starting a level from scratch. It amends the `canvas-scrolling` and `level-size` capabilities where noted.

## Bounded scrolling

While editing, the scroll offset is confined to

```
0 ≤ offset.x ≤ level_size.x
0 ≤ offset.y ≤ level_size.y
```

where `level_size` is the resolved level extent from the `level-size` capability — the explicit size if set, else the background image's size, else the bounding box of all entities, else 1920×1080.

The upper bound is exactly **one screen of slack**. Because the visible region is `offset … offset + viewport`, an offset of `level_size` places the level's far edge at the viewport's *near* edge, leaving a full viewport of empty space beyond the boundary. That space is reachable and usable: entities may still be created in it, and the level can then be enlarged through `Level → Level Size…` to take them in. Expressing the slack as the level size itself makes it one viewport wide at any window size, with no separate constant.

Momentum behaves the same at both ends: on reaching either bound the offset is pinned to it and that axis's velocity is set to zero — no overshoot, no bounce. The bound is evaluated every step, so changing the level size changes the reachable area immediately, and an offset already past a newly shrunk maximum is pulled back to it.

`level_size` is always at least 1 px on each axis, so the range `0 … level_size` can never invert; a level smaller than the viewport is still scrollable over that range.

**Play mode is exempt.** While play is running the editor's scroll model is not stepped at all: the `play-camera` capability owns the offset and may take it outside this range, including negative, to keep the character on screen. Stopping play restores the offset the editor had before, which is inside the bound by construction.

### Amendments

- `canvas-scrolling` states "There is **no** upper bound on either axis: the user can scroll arbitrarily far right or down". While editing there is now the upper bound above. Placement is unaffected: entities may still be created anywhere the view can reach, which is a screen past the boundary.
- `level-size` states "Scrolling and entity placement remain unbounded beyond the boundary". Placement beyond the boundary remains supported, within the slack; scrolling is bounded.

## New Level

`File → New Level` sits at the top of the `File` menu, above `Load Level`. Like `Load Level` it is disabled while play mode is running, with the same explanation on hover, because it would otherwise replace the level being played.

When the editor has unsaved changes, choosing it opens the editor's unsaved-changes confirmation — the same dialog `Exit` uses, which now carries which action is pending — offering:

| Choice | Effect |
| --- | --- |
| Save | Save the current level, then start the new one. If the destination dialog is dismissed, or the write fails, nothing was saved: the confirmation stays open and the level is untouched. |
| Discard | Start the new level, losing the unsaved work. |
| Cancel | Do nothing at all; the level, its size and its background are untouched. |

With no unsaved changes the new level starts immediately, with no prompt.

Starting a new level clears, in memory:

- every entity;
- the explicit level size, so the extent falls back through the resolution order;
- the background image and its recorded size;
- the play spawn point;
- the current selection and any polygon-edit state;
- the undo and redo histories;
- the open `Level Size` dialog, if any, so its `OK` cannot re-apply the old level's explicit size to the new one.

The view returns to the origin, stationary, and the remembered level path is cleared so the next `Save Level` asks for a destination rather than overwriting the level that was previously open. Immediately afterwards the editor reports no unsaved changes, so quitting does not prompt.

`New Level` performs no file I/O: nothing is written, deleted or overwritten. It does not rewrite the editor's config either, so the config still names the last files opened and restarting the editor reloads them; nothing was saved, so nothing is lost, but a restart is not a way to preserve a new level.

## Acceptance

- A1 — The offset is clamped to `0 … level_size` on each axis.
- A2 — At the far bound a full viewport of space is visible past the level's edge.
- A3 — Momentum stops at the far bound with zero velocity, no overshoot or bounce, as at the origin.
- A4 — The bound tracks the level size live, in both directions.
- A5 — Entities can still be placed in the slack beyond the boundary.
- A6 — Play mode's camera is exempt; stopping restores an in-bound offset.
- A7 — A level smaller than the viewport still scrolls and the range never inverts.
- A8 — `New Level` is above `Load Level` and disabled during play with the same hover text.
- A9 — With unsaved changes it prompts Save / Discard / Cancel; Cancel changes nothing, and Save proceeds only if the level was actually written.
- A10 — With no unsaved changes it proceeds without a prompt.
- A11 — It clears entities, level size, background, spawn, selection, polygon-edit state and undo history.
- A12 — It returns the view to the origin.
- A13 — The editor reads as clean immediately afterwards.
- A14 — The remembered level path is cleared, so `Save Level` asks for a destination.
- A15 — Nothing is written to disk.
