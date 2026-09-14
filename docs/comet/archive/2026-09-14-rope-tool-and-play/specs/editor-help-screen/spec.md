# Editor help screen

Complete target behaviour of the in-app help screen.

> Amended 2026-09-14 by the rope-tool-and-play change: the Mouse section gained a Rope-tool row, the Tools section gained the `rope` kind, and the Escape, up/down, left/right and jump rows mention ropes. The completeness guarantee is unchanged.

> Amended 2026-09-14 by the minimap-mouse-navigation change: the Mouse section gained a row for pressing / dragging on the minimap. The completeness guarantee is unchanged.

> Amended 2026-09-12 by the play-combat-simulation change: the Keyboard section gained the attack, shield and debug keys (`J`/`X`, `L`, `F2`) and the `R` row now describes restarting the run.

> Amended 2026-09-12 by the level-size-and-play-simulation change: the Keyboard section gained the play-mode keys, the Mouse section a play-mode click row, and the former four-item "File menu" section became a "Menus" section covering the File, Level, Play and Help menus. The help's guarantee — it lists every key and command the editor handles — is unchanged.

## Opening and closing

- The menu bar has a `Help` menu to the right of `File`. Its single item, `Keyboard & Commands`, opens the help window (and closes the menu).
- `F1` toggles the help window. `?` (`egui::Key::Questionmark`, i.e. Shift+/) toggles it too. Both toggles are ignored while a text field wants keyboard input (`ctx.wants_keyboard_input()`).
- The window's title-bar close button closes it.
- `Escape` has a strict priority order: if the help window is open, one press closes it and that press is fully consumed — it does not cancel a polygon in progress or leave polygon edit mode. If the help window is closed, `Escape` behaves exactly as before (cancel polygon drawing, else exit polygon edit mode).
- Help state (`help_open: bool`) is editor view state: it is not saved to the level file, not restored between runs, and does not affect `has_unsaved_changes()`.

## Presentation

- A floating `egui::Window` titled `Keyboard & Commands`, draggable and resizable, non-modal: while it is open the canvas, toolbox, menus, arrow-key scrolling and every other command keep working; only clicks inside the window are captured by it.
- The body is a vertical `ScrollArea` with a bounded maximum height (≈ 70% of the screen height) so the whole window remains on screen at the editor's minimum size of 800×720 and long content is scrolled rather than clipped.
- Sections appear in this order with a heading each: **Keyboard**, **Mouse**, **Tools**, **Menus**. Each entry is a two-column row: the key / gesture / tool on the left, its effect on the right.

## Keyboard section

Lists every key the input handler responds to, and nothing else:

| Key | Effect |
| --- | --- |
| ← → ↑ ↓ | Scroll the canvas. Hold to accelerate; release to glide to a stop. |
| Enter | Finish the polygon being drawn (needs ≥ 3 vertices). |
| Escape | Close this help; otherwise cancel the polygon or rope being drawn or leave polygon edit mode. |
| Delete / Backspace | Remove the selected vertex of the polygon being edited (a polygon keeps at least 3). |
| Undo | Undo the last change. |
| Redo | Redo the last undone change. |
| F1 / ? | Show or hide this help. |
| F5 | Start or stop play mode. |
| R | Respawn the character at the spawn point (play mode). |
| A / D or ← / → | Move the character left and right; on a rope, pump the swing (play mode). |
| W / S or ↑ / ↓ | Look up and down; down also fast-falls; hold beside a wall or rope to grab and climb it (play mode). |
| Space or Z | Jump, wall jump off a wall, or let go of a rope with a hop (play mode). |
| Shift, K or C | Dash (play mode). |
| J or X | Swing the katana; hold up or down to slash that way (play mode). |
| L | Raise the shield: 2 s invulnerable, then a 5 s cooldown (play mode). |
| F2 | Show or hide the combat debug view (slash hitbox, orc ranges). |

- Undo / Redo labels are produced by `egui::Context::format_shortcut` for `Cmd+Z` and `Shift+Cmd+Z` (`Modifiers::COMMAND`, `Modifiers::COMMAND | Modifiers::SHIFT`), so macOS shows ⌘Z / ⇧⌘Z and other platforms show Ctrl+Z / Shift+Ctrl+Z.

## Mouse section

| Gesture | Effect |
| --- | --- |
| Left click (play mode) | Move the spawn point and respawn the character there. The level is never changed while playing. |
| Left click — Select tool | Select the entity under the cursor; drag to move it. Click empty canvas to deselect. |
| Left click — Delete tool | Remove the entity under the cursor. |
| Left click — polygon tools | Add a vertex at the cursor. |
| Left click — bitmap tools | Place the sprite at the cursor (a translucent preview follows the cursor). |
| Double-click — polygon tool | Close and finish the polygon (needs ≥ 3 vertices). |
| Double-click — Select tool on a polygon | Enter vertex-edit mode for that polygon. |
| Right click — polygon tool | Finish the polygon being drawn. |
| Drag (edit mode) | Move the selected vertex; click a vertex first to select it. |
| Click outside the polygon (edit mode) | Leave vertex-edit mode. |
| Left click — Rope tool | First click sets the top anchor, second click sets the bottom end (Escape cancels). |
| Drag the toolbox | Move the toolbox anywhere on screen. |
| Click / drag the minimap | Jump the view to that part of the level; keep the button down and drag to scroll it live. |

## Tools section

- Generated at runtime from `ToolboxLayout.tools` (`src/toolboxes.json`). Every tool appears exactly once, in file order, as: humanised name (e.g. `coin_tool` → `Coin`, `death_trap_tool` → `Death trap`), its `description`, and its kind (`bitmap` — click to place a sprite; `polygon` — click vertices, Enter / double-click / right-click to finish; `rope` — click the anchor, then click the bottom end; `tool` — a mode).
- A tool of kind `tool` whose name is neither `select_tool` nor `delete_tool` is not implemented by the editor (it currently falls through to Select); its entry ends with **"— not yet implemented"**. This is derived from the same rule the editor uses to map tool names to modes, not from a hand-maintained list.
- If the toolbox layout failed to load, the section shows a single line saying the toolbox could not be loaded, rather than being empty.

## Menus section

Lists every item in every menu, prefixed by its menu name:

| Item | Effect |
| --- | --- |
| File → New Level | Start an empty level; asks whether to save first when there are unsaved changes. |
| File → Background Image | Choose a PNG to show behind the level. |
| File → Load Level | Open a level JSON file. |
| File → Save Level | Write the level JSON file. |
| File → Exit | Quit; asks whether to save first when there are unsaved changes. |
| Level → Level Size… | Set the level's width and height in pixels; saved with the level. |
| Play → Play / Stop | Start or stop play mode (same as F5). |
| Help → Keyboard & Commands | Show this window. |

## Structure

- Content is built by a pure `help` module: `HelpContent::build(tools: Option<&[ToolDef]>, undo_label: &str, redo_label: &str) -> HelpContent` with `keyboard`, `mouse`, `tools`, `file` entry lists, plus `humanize_tool_name` and `is_tool_implemented` helpers, and `escape_closes_help(help_open) -> bool`. The module has no egui `Context` / `Ui` dependency so completeness is unit-tested.
- `main.rs` owns `help_open`, renders the window from `HelpContent`, and wires the menu item, `F1`, `?` and the Escape priority.

## Acceptance

- A1 — `Help → Keyboard & Commands` opens the help window.
- A2 — `F1` and `?` each toggle the help window.
- A3 — With help open, `Escape` closes it and does not cancel a polygon / edit mode; with help closed, `Escape` behaves as before.
- A4 — The window is closable, draggable and non-modal; arrow-key scrolling and canvas clicks keep working while it is open.
- A5 — Keyboard section lists every key the editor handles — Arrow keys, Enter, Escape, Delete/Backspace, Undo, Redo, F1 / ?, F5, R, the play-mode movement keys and the combat keys `J`/`X`, `L` and `F2` — and nothing the editor does not handle.
- A6 — Undo/Redo labels come from `format_shortcut` (⌘ on macOS, Ctrl elsewhere).
- A7 — Mouse section covers left click per tool (including the Rope tool's two clicks), both double-click meanings, right-click, entity / vertex / toolbox drag, click-outside-to-exit-edit, and click / drag on the minimap.
- A8 — Tools section lists every `toolboxes.json` tool exactly once with description and kind (the `rope` kind reads `click the anchor, then click the bottom end`); `add_point_tool` / `remove_point_tool` flagged "not yet implemented"; `select_tool` / `delete_tool` not flagged.
- A9 — Menus section lists every item of the File, Level, Play and Help menus, noting Exit's unsaved-changes prompt.
- A10 — Body is in a bounded vertical `ScrollArea`; window fits at 800×720.
- A11 — Help state is view-only: not serialised, no effect on unsaved-changes.
