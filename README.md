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
  character grabs it, swings, climbs along it and lets go with momentum. The rope is a
  flexible chain: the tail dangles and trails the swing, and a free rope ripples and settles.
- **Two-axis smooth scrolling**: hold the arrow keys to scroll in any direction; the canvas
  accelerates and glides to a stop.
- **Explicit level size**: set the level's width and height, drawn as a boundary and saved
  with the level. Scrolling is deliberately unbounded, so the level can be grown later.
- **Background image**: load a PNG behind the level for reference; large images are tiled.
- **Play mode**: drop a controllable character into the level and test it — full movement
  (run, jump, dash, wall slide, wall jump, climb, rope swing) plus combat (katana, orcs, coins, death
  pits, shield). Play never modifies the level.
- **AI level generation**: describe an area in plain words and have an AI model write a whole
  Hollow-Knight-style level — chambers, corridors, hazards, enemies and rewards. The editor
  will not hand you the level until its own play simulation has actually walked a route from
  the start to the end, so a generated level is always playable. OpenAI by default; point the
  endpoint at any OpenAI-compatible server (Ollama, LM Studio, llama.cpp, vLLM) to use a model
  on your own machine instead.
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
| Level → Generate Level with AI… | Have an AI model write a level — OpenAI by default, or a local model by changing the endpoint — proven playable before it is delivered. Asks whether to save first when there are unsaved changes. |
| Level → Level Size… | Set the level's width and height in pixels; saved with the level. |
| Play → Play / Stop | Start or stop play mode (same as F5). |
| Help → Keyboard & Commands | Show the help window. |

### Generating a level with AI

`Level → Generate Level with AI…` asks an AI model to write a whole area.

**Set your API key first.** The editor reads it from the `OPENAI_API_KEY` environment
variable and nowhere else: it never stores the key, never logs it and never shows it. In the
terminal you launch the editor from:

```zsh
read -rs "OPENAI_API_KEY?OpenAI API key: " && export OPENAI_API_KEY && echo
```

(That form keeps the key out of your shell history.) To keep it across terminals, put it in
the macOS keychain once and read it from `~/.zshrc`:

```zsh
security add-generic-password -a "$USER" -s openai-api-key -w
# then, in ~/.zshrc:
export OPENAI_API_KEY="$(security find-generic-password -s openai-api-key -w 2>/dev/null)"
```

It only reaches the editor when you launch it from a terminal, which `cargo run` is. Launched
from Finder, the editor will correctly report that it found no key.

Then the dialog:

| Setting | Default | Notes |
| --- | --- | --- |
| Endpoint | `https://api.openai.com/v1` | Any OpenAI-compatible server. Put `http://localhost:11434/v1` here to use a local model instead; a local endpoint needs no key and is sent none. |
| Model | `gpt-6-astra` | Any model id the endpoint serves. |
| Chambers | 5 | How many chambers the area should have. |
| Seed | *(none)* | Passed to endpoints that honour one, so a level can be reproduced. |

The dialog tells you whether it found a key — never the key itself — and `Generate` stays
unavailable while a needed key is missing, rather than starting a request that can only fail.

**What leaves your machine:** with the default endpoint, your description and the generation
prompts are sent to OpenAI. Point the endpoint at a local server and nothing leaves at all.

The model writes the level JSON itself, coordinate by coordinate, in staged requests — the
shape of the area first, then one request per chamber — so the window shows which chamber it
is on and can be cancelled at any point. Nothing touches your open level until a level is
ready.

What comes back is checked before you ever see it. Every entity must be one of the editor's
own tools, and then the editor drives its play simulation from the level's spawn to its
exit — the real simulation, the one behind play mode — and only a level it can actually walk
through is delivered. If the route does not work, the model is told exactly where it broke
and asked again, twice, re-asking only the chamber the route died in. If it still does not
work, the editor adds the smallest ledges that close the route and tells you where it did so;
the model's own geometry is never moved or deleted. If even that fails, generation stops and
your open level is left alone.

A generated level replaces the one you have open, in a single undoable step, so Ctrl+Z puts
the old one back.

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
  "spawn": [300.0, 1100.0],
  "exit": [3600.0, 1155.0],
  "entities": [ ... ]
}
```

`level_size` is the level's explicit extent, written when one has been set. `spawn` and
`exit` are where a run begins and where its critical path ends; a generated level carries
both, a hand-built one carries neither, and play mode starts the character at `spawn` when
it is there. All three are optional: files saved before they existed load unchanged.

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
