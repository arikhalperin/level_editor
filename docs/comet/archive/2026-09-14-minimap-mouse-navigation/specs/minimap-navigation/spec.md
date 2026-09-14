# Minimap navigation

Complete target behaviour of the bottom-right navigation box (the minimap) as a mouse control. The minimap's drawing (thumbnail, yellow viewport rectangle, placement and sizing) is described in `canvas-scrolling`; this capability adds input to it.

## Layout

- A pure function computes the minimap's on-screen rectangle (`map_rect`) and its inner image rectangle (`inner`, `map_rect` shrunk by the 4 px frame) from the central panel rect and the resolved level extent, using exactly the sizing rules the renderer uses today (16 px margin, 220 px longest edge, 100 px minimum short edge, clamped to the panel and screen). The renderer and the hit-test both call this function in the same frame, so what is drawn and what is clickable are the same rectangle.
- `inner` maps linearly to level space: minimap point `p` ↦ world `((p.x − inner.min.x) / inner.width() · level.x, (p.y − inner.min.y) / inner.height() · level.y)`.

## Gesture

- **Press.** A left-button press whose position lies inside `map_rect` while editing starts a minimap gesture. The view is re-centred on the corresponding world point immediately (this frame).
- **Drag.** While the button stays down after such a press, every frame re-centres the view on the world point under the current pointer position, using the same mapping, even if the pointer has moved outside `map_rect`. Positions are clamped to `inner` before mapping so the drag can pin the view to a level edge.
- **Release.** Releasing the button ends the gesture. A press that started outside `map_rect` (on the canvas, toolbox, a window or menu) never becomes a minimap gesture, even if it passes over the minimap.
- Press and drag on the 4 px frame count as being on the minimap (they are inside `map_rect`) and map to the nearest `inner` edge.

## Effect on the view

- Re-centring sets `scroll.offset = world − panel_size / 2` and then clamps each axis to `0 ..= level_size` (the same reachable range as arrow-key scrolling, see `canvas-scrolling`). The result is applied instantly; no easing.
- `scroll.velocity` is set to zero on the press and remains zero throughout the gesture, so any arrow-key momentum is discarded and the view does not glide after release.
- `scroll_offset` (the integer pixel offset every renderer reads) is refreshed in the same frame, so the yellow viewport rectangle and the canvas move together.
- A repaint is requested every frame while the gesture is active so the drag feels continuous.

## Interaction with the rest of the editor

- A press on the minimap is consumed: it does not place a sprite, add a polygon vertex, select, deselect, start dragging or delete an entity, enter or leave vertex-edit mode, or move the play spawn point. Canvas clicks outside `map_rect` are unchanged.
- Level data is never modified; `has_unsaved_changes()` is unaffected; nothing is written to the level file.
- While play mode is running, the minimap ignores presses and drags (the play camera owns the offset). The press is still consumed so it cannot relocate the spawn point.
- While hovering `map_rect` during editing the cursor is the pointing hand (`egui::CursorIcon::PointingHand`); during a drag it is the grabbing hand.
- Arrow-key scrolling keeps working before and after the gesture; a key held during a drag does not fight the drag, because the drag re-applies its target after the scroll model steps.

## Help

- The Mouse section of the help window lists the gesture as `Click / drag the minimap` → `Jump the view to that part of the level; keep the button down and drag to scroll it live.` (see `editor-help-screen`).

## Acceptance

- A1 — Editing, level 25600×720, view at the origin: a press at the horizontal middle of the minimap centres the view on world x ≈ 12800 (`scroll_offset.x ≈ 12800 − panel_width/2`) on the next frame; the yellow rectangle surrounds the pressed spot.
- A2 — A press whose target centre is within half a viewport of the origin yields `scroll_offset` exactly 0 on that axis; a press on the far edge puts that level edge at the centre of the view (`scroll_offset = level_size − panel_size/2` on that axis); the offset is never outside `0 ..= level_size`.
- A3 — Press-and-drag across the minimap moves the view every frame while the button is held, also once the pointer has left the minimap; release ends it; a drag started on the canvas that passes over the minimap does not move the view.
- A4 — Scroll velocity is zero at the press and after release: no glide, and pending arrow-key momentum is discarded.
- A5 — With Bitmap, Polygon, Select or Delete active, a minimap click changes no entities, vertices or selection; canvas clicks outside the minimap behave as before.
- A6 — While playing, minimap presses and drags change neither the view nor the spawn point.
- A7 — Hovering the minimap while editing shows the pointing-hand cursor; a press leaves level data and `has_unsaved_changes()` unchanged.
- A8 — The help's Mouse section has the minimap row and the help module's completeness test asserts it.
- A9 — `cargo test` passes with unit tests for the pure layout + mapping (centre, clamps at both ends, both axes) and for the help row.
