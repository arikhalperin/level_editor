# Canvas scrolling

> Amended 2026-09-12 by the play-camera-containment change: the non-negative offset invariant holds **while editing**. Play mode's camera owns the offset and may take it negative so the character stays on screen; see the `play-camera` capability.

Complete target behaviour of the level-editor canvas viewport after this change is archived.

## Coordinate model

- The canvas shows a level in world coordinates anchored at the top-left origin (0, 0), Y-down. This matches egui's convention; the Bevy integration flips Y against `background_height` on import.
- The viewport is described by `scroll_offset: Vec2` — the world position visible at the top-left corner of the central canvas panel.
- `screen = world − round(scroll_offset)` and `world = screen + scroll_offset`.
- While editing, `scroll_offset.x ≥ 0` and `scroll_offset.y ≥ 0`. There is **no** upper bound on either axis: the user can scroll arbitrarily far right or down to place entities beyond the background image or beyond any existing entity. While play mode is running the camera owns the offset and may take it negative, so a character at negative world coordinates stays on screen; `ScrollModel` is not stepped then, and its own origin clamp is unchanged.

## Input

- `ArrowLeft` / `ArrowRight` scroll horizontally (−x / +x). `ArrowUp` / `ArrowDown` scroll vertically (−y / +y). Opposite keys held together cancel on that axis; perpendicular keys combine into diagonal scrolling.
- Only arrow keys scroll the canvas. Mouse wheel, trackpad gestures and drag-to-pan do not scroll (out of scope for this capability).
- Arrow keys do not scroll while a text field has keyboard focus (egui's `wants_keyboard_input`), preserving existing text-entry behaviour.

## Motion model

The scroll model is a pure struct independent of egui, advanced once per frame with an explicit `dt` (seconds):

- **Acceleration while held.** For each axis with a held direction, velocity accelerates toward `±MAX_SPEED` at a fixed rate `ACCEL` (px/s²), reaching full speed in roughly 0.25 s. `MAX_SPEED` ≥ the previous fixed 600 px/s.
- **Momentum on release.** When no key is held on an axis, that axis's velocity decays exponentially (`v *= exp(−DECAY · dt)`) and is snapped to exactly 0 once `|v|` falls below a small threshold (`STOP_SPEED`, ≈ 5 px/s). Constants are chosen so a full-speed release comes to rest in ≤ 1.5 s. There is no perpetual drift.
- **Integration.** `offset += velocity · dt` each frame, then clamped: any component `< 0` is set to 0 and that axis's velocity is set to 0 (momentum stops at the origin; no bounce, no overshoot).
- **Determinism.** Given the same sequence of `(held_dirs, dt)` the model produces the same offsets; this is what the unit tests exercise.

## Repaint policy

- While any arrow key is held **or** either velocity component is non-zero, the app calls `ctx.request_repaint()` every frame so the motion advances at display rate rather than OS key-repeat rate.
- When the canvas is at rest (no key held, zero velocity), no repaint is requested by the scroll system; the editor returns to egui's reactive repainting.
- `dt` comes from `ctx.input(|i| i.stable_dt)`.

## Rendering alignment

- Each frame computes one `pixel_offset = scroll_offset.round()` (an integer-valued `Vec2`).
- The background painter, every entity draw, the in-progress polygon preview, selection outlines and the placement ghost all subtract this same `pixel_offset`. No renderer truncates or floors independently, so the background never shimmers relative to entities.
- Mouse→world conversions for placement, polygon vertices, hit-testing and dragging use `screen + pixel_offset` so that what the user clicks matches what is drawn.

## Background rendering

- `BackgroundImageController::draw` takes the full `Vec2` pixel offset and draws the image at screen position `(−offset.x, −offset.y)` in 1:1 pixels.
- The image is split into a 2-D grid of tiles. Tile width and height are each `min(dimension, 8192)`, so no texture exceeds 8192 px in either axis. Tiles are addressed by `(col, row)`.
- Only tiles intersecting the visible panel, plus a one-tile margin on each side, are uploaded and drawn; tiles that scroll out of that window are released. Tiles already resident for the current window are reused, never re-uploaded.
- Areas of the canvas beyond the image's right or bottom edge (reachable because scrolling is unbounded) show the panel's plain background colour; no tile is drawn there and no error is logged.
- When no background image is loaded, nothing is drawn by the background painter and scrolling behaves identically.
- The minimap thumbnail is unaffected by tiling.

## Minimap

- The minimap's viewport rectangle is positioned from `scroll_offset` on both axes and sized from the panel dimensions, scaled to the minimap. It is clipped to the minimap bounds when the viewport is beyond the level's known extent (background size, or entity bounding box when no background is loaded).

## Persistence

- `scroll_offset` is view state only; it is not written to the level file. The level format (`LevelData` with optional `background_size`) is unchanged by this capability.
- The debug export continues to record the current `scroll_offset` for both axes.

## Acceptance

- A1 — Holding `ArrowDown` increases `scroll_offset.y` and moves background and entities upward on screen; `ArrowUp` reverses.
- A2 — Holding `ArrowRight` / `ArrowLeft` increases / decreases `scroll_offset.x` exactly as before.
- A3 — Scrolling continues past the background's right/bottom edges and past all entities, with a 25600×720 background and with no background at all; a bitmap placed in that space receives a world position beyond those extents.
- A4 — While editing, `scroll_offset` components are never negative; momentum toward the origin stops exactly at 0. (Play mode's camera is the stated exception.)
- A5 — A repaint is requested every frame while a key is held or velocity is non-zero.
- A6 — After release, velocity decays to exactly zero within 1.5 s and never re-accelerates on its own.
- A7 — Background and entities use one shared rounded integer-pixel offset per frame.
- A8 — Background draws offset by `−scroll_offset.y`; no tile exceeds 8192 px in either dimension; an image taller than 8192 px yields more than one tile row.
- A9 — Mouse→world for placement, vertices and hit-testing equals `screen + offset` on both axes.
- A10 — The minimap viewport rectangle tracks `scroll_offset` on both axes.
