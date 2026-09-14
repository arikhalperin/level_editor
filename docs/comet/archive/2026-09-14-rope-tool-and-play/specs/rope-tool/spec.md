# Rope tool

Complete target behaviour of ropes as editor entities: the tool that places them, how they are drawn and edited, and how they are saved.

## Toolbox

- `src/toolboxes.json` gains a tool `rope_tool` of kind `rope` ("Tool to hang a rope on the map.", icon `assets/rope.png`, bundled as `src/assets/rope.png`) and the main toolbox gains a fifth row holding it. Rows may have fewer cells than the widest row.
- Selecting it puts the editor in the Rope tool mode. Switching to any other tool cancels a rope in progress.

## Placement

- First left click on the canvas sets the rope's anchor at the click's world position.
- While the anchor is set and the bottom end is not, a preview line is drawn from the anchor straight down to the cursor's height, and the anchor is marked.
- Second left click at a world `y` at least `ROPE_MIN_LENGTH` (60 px) below the anchor finalises a rope with `length = click.y − anchor.y`; the click's `x` is ignored. A second click above the anchor or closer than the minimum does nothing (the rope stays in progress).
- `Escape` cancels the rope in progress (after the help window, before polygon handling, in the same priority list). Switching tools or entering play mode cancels it too.
- Placement calls `save_state()` once per finished rope, so undo removes the rope.
- Clicks that land on the toolbox, a floating window, the menu bar or the minimap are consumed by those, as for every other tool.

## Canvas

- A rope is drawn as a `ROPE_THICKNESS` 6 px line in a rope colour from its anchor to `anchor + (0, length)`, with a small disc at the anchor. Selected ropes get the same green outline treatment other entities use (the line drawn thicker in green).
- Hit-test: a point is on the rope when its distance to the segment is at most 8 px. The Select tool selects and drags the whole rope (the anchor moves by the drag delta; length is unchanged). The Delete tool removes it. Double-click does nothing special on a rope.
- Length cannot be edited after placement.

## Level file

- `Entity::Rope { anchor, length }` serialises to a bitmap entry: `{"type":"bitmap","position":[anchor.x, anchor.y],"bitmap_name":"rope_tool","size":[ROPE_THICKNESS, length]}`.
- Loading a bitmap entry whose `bitmap_name` is `rope_tool` yields a rope with `anchor = position` and `length = max(size[1], ROPE_MIN_LENGTH)`. Any other bitmap entry loads as before. The schema (`LevelData`, `LevelEntity`) is unchanged.
- `LEVEL_FORMAT.md` and `BEVY_INTEGRATION.md` document the `rope_tool` entry; `README.md` lists ropes under features and the rope clicks under usage.

## Acceptance

- A1 — Two clicks (anchor, then a point ≥ 60 px lower) create one rope with the anchor at the first click and the length from the second click's height; a click above or too close does nothing; Escape or a tool switch cancels; a preview follows the cursor in between.
- A2 — The rope is drawn as a vertical line with an anchor knob; Select picks it within 8 px of the line and drags it whole; Delete removes it; undo and redo cover place, move and delete.
- A3 — Save writes the `rope_tool` bitmap entry with `size = [6, length]`; load restores it; files without ropes load unchanged; the schema is untouched.
- A4 — The toolbox shows the rope tool in its own row and the help lists it as a `Rope` tool with the hint `click the anchor, then click the bottom end`.
