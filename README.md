# Rust Level Editor

A 2D level editor built in Rust with egui/eframe for authoring platformer levels, with a
built-in play mode so a level can be tested without leaving the editor.

## Features

- **Floating toolbox**: select, erase, polygon and sprite-placement tools, draggable anywhere.
- **Polygon drawing**: point-by-point creation and editing.
  - Click to add points; Enter, double-click or right-click to finish; Escape to cancel.
  - Double-click a polygon to edit it; drag its points; Delete/Backspace to remove one.
- **Entity placement**: place sprites (coins, orcs, death traps) anywhere in the level.
- **Ropes**: hang a rope with two clicks (anchor, then bottom end); in play mode the
  character grabs it, swings, climbs along it and lets go with momentum.
- **Two-axis smooth scrolling**: hold the arrow keys to scroll in any direction; the canvas
  accelerates and glides to a stop.
- **Explicit level size**: set the level's width and height, drawn as a boundary and saved
  with the level. Scrolling is deliberately unbounded, so the level can be grown later.
- **Background image**: load a PNG behind the level for reference; large images are tiled.
- **Play mode**: drop a controllable character into the level and test it — full movement
  (run, jump, dash, wall slide, wall jump, climb, rope swing) plus combat (katana, orcs, coins, death
  pits, shield). Play never modifies the level.
- **Minimap**: a thumbnail with the current viewport marked. Click or drag on it to jump the view to that part of the level.
- **Undo/redo**: Cmd/Ctrl+Z and Shift+Cmd/Ctrl+Z.
- **In-app help**: press F1 for a window listing every key, gesture, tool and menu item.
- **Save/load**: a small JSON format describing entity positions and types.

## How to Run

1. Install Rust: https://rustup.rs/
2. Run: `cargo run`

Run it from the repository root — the toolbox definition is read from `src/toolboxes.json`
relative to the working directory.

## Usage

Press **F1** in the editor for the same reference, generated from the running build.

### Editing

| Key | Effect |
| --- | --- |
| ← → ↑ ↓ | Scroll the canvas. Hold to accelerate; release to glide to a stop. |
| Enter | Finish the polygon being drawn (needs at least 3 vertices). |
| Escape | Close the help; otherwise cancel the polygon or leave polygon edit mode. |
| Delete / Backspace | Remove the selected vertex of the polygon being edited. |
| Cmd/Ctrl+Z | Undo. |
| Shift+Cmd/Ctrl+Z | Redo. |
| F1 or ? | Show or hide the help window. |

| Mouse | Effect |
| --- | --- |
| Left click, Select tool | Select the entity under the cursor; drag to move it. |
| Left click, Delete tool | Remove the entity under the cursor. |
| Left click, polygon tool | Add a vertex. |
| Left click, sprite tool | Place the sprite at the cursor. |
| Left click, Rope tool | First click sets the anchor, second click sets the bottom end; Escape cancels. |
| Double-click, polygon tool | Close and finish the polygon. |
| Double-click, Select tool | Enter vertex-edit mode for that polygon. |
| Right click, polygon tool | Finish the polygon being drawn. |
| Drag the toolbox | Move it anywhere on screen. |

### Play mode

| Key | Effect |
| --- | --- |
| F5 | Start or stop play mode. |
| R | Restart the run: character back at the spawn point, orcs and coins restored. |
| Left click | Move the spawn point and respawn there. |
| A / D or ← / → | Move left and right. |
| W / S or ↑ / ↓ | Look up and down; down also fast-falls and climbs down. |
| Space or Z | Jump, and wall jump off a wall. |
| Shift, K or C | Dash. |
| J or X | Swing the katana; hold up or down to slash that way. |
| L | Raise the shield: 2 s invulnerable, then a 5 s cooldown. |
| F2 | Show or hide the combat debug view (slash hitbox, orc ranges). |

The character's movement and combat use the tuning of the reference game at
`/Users/arikha/bevy_prince_platformer`, transcribed constant for constant into
[`src/game_config.rs`](src/game_config.rs). It is an independent kinematic
reimplementation rather than that game's physics, so the feel is close but not identical.

### Menus

| Item | Effect |
| --- | --- |
| File → Background Image | Choose a PNG to show behind the level. |
| File → Load Level | Open a level JSON file. |
| File → Save Level | Write the level JSON file. |
| File → Exit | Quit; asks whether to save first when there are unsaved changes. |
| Level → Level Size… | Set the level's width and height in pixels; saved with the level. |
| Play → Play / Stop | Start or stop play mode (same as F5). |
| Help → Keyboard & Commands | Show the help window. |

### Tools

Tools come from [`src/toolboxes.json`](src/toolboxes.json); editing that file changes the
toolbox and the help without touching any code.

| Tool | Kind | Notes |
| --- | --- | --- |
| Select | mode | Select and move entities and polygons. |
| Delete | mode | Click an entity to remove it. |
| Polygon | polygon | A plain polygon. |
| Wall | polygon | Climbable in play mode, and by the game. |
| Blocker | polygon | Solid, not climbable. |
| Coin | sprite | Collectible; worth 40 points in play mode. |
| Orc | sprite | An enemy in play mode. |
| Death trap | sprite | Kills on contact in play mode. |
| Add point / Remove point | mode | Present in the toolbox but not yet implemented. |

## Level Format

Levels are JSON describing **only** entity positions, sizes and types. Colliders and physics
properties are not stored — the consuming game derives them from the entity type. See
[LEVEL_FORMAT.md](LEVEL_FORMAT.md) for the full schema.

```json
{
  "version": "1.0",
  "background": null,
  "background_size": [1920.0, 1080.0],
  "level_size": [4000.0, 2000.0],
  "entities": [ ... ]
}
```

`level_size` is the level's explicit extent, written when one has been set. It is optional:
files saved before it existed load unchanged.

### Coordinates

The editor works in a top-left origin with Y increasing **downward**. A game using a Y-up
world must flip Y, and must take the height from `level_size` rather than assuming a fixed
viewport:

```rust
let level_height = level.level_size.map(|s| s[1]).unwrap_or(720.0);
let world = Vec2::new(pos[0], level_height - pos[1]);
```

> The reference game at `/Users/arikha/bevy_prince_platformer` still flips against a
> hard-coded `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0`, so any level whose height is not 720
> loads there vertically offset until that constant reads from the file.

## Integration

[LEVEL_FORMAT.md](LEVEL_FORMAT.md) and [BEVY_INTEGRATION.md](BEVY_INTEGRATION.md) show how to
load the format into Bevy. Both are written against **Avian2D**, while the reference game
actually uses **bevy_rapier2d** — the JSON itself is engine-agnostic, but the collider code
in those guides needs translating if you follow the game rather than the docs.

## Notes

- Toolbox and entity icons are placeholders. Replace them with your own assets as needed.
- Sprite entities are given box colliders by the game; polygons become convex hulls, so a
  concave polygon drawn here collides as its hull.
- Play mode is strictly non-destructive: defeating orcs, collecting coins and dying change
  nothing in the level and never mark it as having unsaved changes.
