# Outcome

The bottom-right navigation box (the minimap) becomes a mouse control: pressing on a spot in it puts that part of the level in the middle of the canvas at once, and dragging across it keeps moving the view live, so a user crosses a very wide level in one gesture instead of holding an arrow key.

# Scope

- Left-button press and drag on the minimap while editing (play mode not running).
- Mapping a minimap point to a world position, centring the view on it, clamped to the same reachable range arrow-key scrolling uses (`0 ..= level_size` per axis).
- Presses that land on the minimap never reach the canvas underneath (they must not place, select or delete entities, nor move the play spawn).
- The help window's Mouse section gains a row for the new gesture, keeping its "every gesture is listed" guarantee.
- The README's Minimap bullet mentions the gesture.

# Non-goals

- Mouse wheel, trackpad gestures or drag-to-pan on the canvas itself (still keyboard-only, as the canvas-scrolling spec states).
- Changing the minimap's size, placement, thumbnail or the yellow viewport rectangle's geometry.
- Any change to play mode's camera, which keeps owning the view while playing.
- Animated / eased movement of the view.

# Acceptance examples

- A1 — Editing, level 25600×720, view at the origin: press the left button at the horizontal middle of the minimap → on the next frame the view is centred on world x ≈ 12800 (`scroll_offset.x ≈ 12800 − panel_width/2`) and `scroll_offset.y` is clamped to its reachable range; the yellow rectangle now surrounds the pressed spot.
- A2 — Press near the minimap's left / top edge (target centre less than half a viewport from the origin) → `scroll_offset` is exactly 0 on that axis, never negative. Press on the minimap's far edge → that level edge sits at the centre of the view (`scroll_offset = level_size − panel_size/2` on that axis); the offset never exceeds `level_size`, the same bound arrow-key scrolling has.
- A3 — Press and, without releasing, drag across the minimap → the view follows the pointer every frame while the button is held, including when the pointer leaves the minimap rectangle mid-drag; releasing ends the gesture. A drag that started on the canvas and merely passes over the minimap does not move the view.
- A4 — After any minimap press or drag the scroll velocity is zero: the view does not glide on after the button is released, and pending arrow-key momentum is discarded at the moment of the press.
- A5 — With the Bitmap, Polygon, Select or Delete tool active, a click on the minimap adds no entity, adds no polygon vertex, changes no selection and deletes nothing; a click on the canvas outside the minimap still behaves as before.
- A6 — While play mode is running, pressing or dragging on the minimap changes neither the view nor the spawn point; the camera keeps following the character.
- A7 — Hovering the minimap while editing shows a pointing-hand cursor; a press does not change any level data and `has_unsaved_changes()` is unaffected.
- A8 — Help → Keyboard & Commands: the Mouse section has a row for pressing / dragging on the minimap, and the help module's completeness test asserts it.
- A9 — `cargo test` passes, including unit tests for the pure minimap layout + point-to-offset mapping (centre, clamp at both ends, both axes) and the help row.

# Constraints and invariants

- The minimap is drawn with `ctx.debug_painter()` (layer `Order::Debug`, no `Area`), so today a click on it falls straight through to the canvas; `pointer_over_ui` does not see it.
- Pointer state is read once per frame before the central panel is shown; the minimap is drawn last inside the central panel. The minimap's on-screen rectangle is therefore computed by a pure layout function from the panel rect and level extent, available both for hit-testing and for drawing, so the two never disagree.
- While editing, `scroll_offset` stays within `0 ..= level_size` per axis; the `ScrollModel` clamp is unchanged.
- Rendering keeps using the single per-frame `pixel_offset`.
- The minimap remains view-only state: nothing about it is serialised.

# Decisions

- D1 (Q1) — Gesture is press **and drag**: a press jumps the view, and holding the button while moving keeps re-centring the view on the pointer each frame until release. The gesture is captured at press time on the minimap only; the pointer may leave the minimap during the drag.
- D2 (Q2) — The pressed minimap point becomes the **centre** of the view: `scroll_offset = world_point − panel_size/2`, then clamped to `0 ..= level_size` per axis.
- D3 (Q3) — The view moves **instantly**: the new offset is applied the same frame and scroll velocity is set to zero (no glide, no residual momentum).
- D4 (Q4) — While play mode runs the minimap **ignores the mouse** entirely; the press is still consumed so it cannot move the spawn point.
- D5 — Presses over the minimap are consumed by it and never reach the canvas tools.
- D6 — The help window's Mouse section lists the gesture (required by the editor-help-screen spec's completeness guarantee).
- D7 — The pointer shows a pointing-hand cursor over the minimap while editing, as hover feedback.

# Open questions

None. Shared understanding confirmed by the user on 2026-09-14.

# Verification expectations

- `cargo test` passes, including new unit tests for the minimap layout / point-to-offset mapping and the help-content completeness test.
- `cargo build` is warning-free for the touched code.
- A manual run confirms the click-and-drag feel on the bundled 25600×720 level.
