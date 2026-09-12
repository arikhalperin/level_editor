# Level size

Complete target behaviour of the editor's explicit level size after this change is archived.

## Model

The level has an extent in world pixels, anchored at the top-left origin (0, 0) and running to (width, height). It is resolved in this priority order: the explicitly set size, else the loaded background image's size, else the bounding box of all entities, else the 1920×1080 default. A size is valid only when both components are finite and within 1 … 1 000 000 px; anything else is refused and the previous size kept.

## Setting the size

A `Level` menu in the menu bar carries a `Level Size…` item that opens a modal dialog. The dialog pre-fills Width and Height from the currently resolved size and offers OK and Cancel. OK applies a valid size and closes; an invalid entry leaves the size unchanged. Cancel closes without applying. The dialog is the only way to change the size in this capability — there is no drag handle on the canvas.

## Effect on the editor

The level boundary is drawn on the canvas as a thin outline from world (0, 0) to (width, height), offset by the shared scroll pixel offset like every other element, so it scrolls with the level and marks the editable area.

The minimap computes its thumbnail area and viewport rectangle against the resolved level size.

The size does **not** clamp scrolling. The canvas still scrolls arbitrarily far right and down, and entities may still be placed beyond the boundary, preserving the behaviour confirmed in the two-axis-smooth-scrolling change; the boundary is a marker, and the level can be grown afterwards to take in what was placed outside it.

## Persistence

`LevelData` carries `level_size: Option<[f32; 2]>`, serialised on save when a size has been set and read back on load. The field is optional with a serde default, so a level file written before this change loads unchanged and without error, falling back through the resolution order above.

Consumers of the level JSON outside this editor must read the height from `level_size` rather than assuming a fixed viewport height. The Bevy game at `/Users/arikha/bevy_prince_platformer` currently flips editor Y with the hard-coded constant `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0` in `src/game/config.rs`, so any level whose height is not 720 will be vertically offset there until that constant is replaced by the value from the file. Making that change in the game is out of scope for this capability; writing the field correctly is not.

## Acceptance

- A1 — `Level → Level Size…` opens a dialog pre-filled from the current size; OK applies new values, Cancel leaves them unchanged.
- A2 — Resolution order is explicit size, then background size, then entity bounding box, then 1920×1080.
- A3 — `level_size` round-trips through save and load, and a file lacking the field loads unchanged.
- A4 — A boundary outline is drawn from (0,0) to (width, height) and scrolls with the level.
- A5 — The minimap uses the resolved level size as its extent.
- A6 — Non-finite, zero, negative or above-1 000 000 values are refused and the previous size kept.
- A7 — Scrolling and entity placement remain unbounded beyond the boundary.
