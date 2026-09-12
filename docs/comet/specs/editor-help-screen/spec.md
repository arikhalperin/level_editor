# Editor help screen

Complete target behaviour of the in-app help screen after this change is archived.

## Opening and closing

- The menu bar has a `Help` menu to the right of `File`. Its single item, `Keyboard & Commands`, opens the help window (and closes the menu).
- `F1` toggles the help window. `?` (`egui::Key::Questionmark`, i.e. Shift+/) toggles it too. Both toggles are ignored while a text field wants keyboard input (`ctx.wants_keyboard_input()`).
- The window's title-bar close button closes it.
- `Escape` has a strict priority order: if the help window is open, one press closes it and that press is fully consumed — it does not cancel a polygon in progress or leave polygon edit mode. If the help window is closed, `Escape` behaves exactly as before (cancel polygon drawing, else exit polygon edit mode).
- Help state (`help_open: bool`) is editor view state: it is not saved to the level file, not restored between runs, and does not affect `has_unsaved_changes()`.

## Presentation

- A floating `egui::Window` titled `Keyboard & Commands`, draggable and resizable, non-modal: while it is open the canvas, toolbox, menus, arrow-key scrolling and every other command keep working; only clicks inside the window are captured by it.
- The body is a vertical `ScrollArea` with a bounded maximum height (≈ 70% of the screen height) so the whole window remains on screen at the editor's minimum size of 800×720 and long content is scrolled rather than clipped.
- Sections appear in this order with a heading each: **Keyboard**, **Mouse**, **Tools**, **File menu**. Each entry is a two-column row: the key / gesture / tool on the left, its effect on the right.

## Keyboard section

Lists every key the input handler responds to, and nothing else:

| Key | Effect |
| --- | --- |
| ← → ↑ ↓ | Scroll the canvas. Hold to accelerate; release to glide to a stop. |
| Enter | Finish the polygon being drawn (needs ≥ 3 vertices). |
| Escape | Close this help; otherwise cancel the polygon being drawn or leave polygon edit mode. |
| Delete / Backspace | Remove the selected vertex of the polygon being edited (a polygon keeps at least 3). |
| Undo | Undo the last change. |
| Redo | Redo the last undone change. |
| F1 / ? | Show or hide this help. |

- Undo / Redo labels are produced by `egui::Context::format_shortcut` for `Cmd+Z` and `Shift+Cmd+Z` (`Modifiers::COMMAND`, `Modifiers::COMMAND | Modifiers::SHIFT`), so macOS shows ⌘Z / ⇧⌘Z and other platforms show Ctrl+Z / Shift+Ctrl+Z.

## Mouse section

| Gesture | Effect |
| --- | --- |
| Left click — Select tool | Select the entity under the cursor; drag to move it. Click empty canvas to deselect. |
| Left click — Delete tool | Remove the entity under the cursor. |
| Left click — polygon tools | Add a vertex at the cursor. |
| Left click — bitmap tools | Place the sprite at the cursor (a translucent preview follows the cursor). |
| Double-click — polygon tool | Close and finish the polygon (needs ≥ 3 vertices). |
| Double-click — Select tool on a polygon | Enter vertex-edit mode for that polygon. |
| Right click — polygon tool | Finish the polygon being drawn. |
| Drag (edit mode) | Move the selected vertex; click a vertex first to select it. |
| Click outside the polygon (edit mode) | Leave vertex-edit mode. |
| Drag the toolbox | Move the toolbox anywhere on screen. |

## Tools section

- Generated at runtime from `ToolboxLayout.tools` (`src/toolboxes.json`). Every tool appears exactly once, in file order, as: humanised name (e.g. `coin_tool` → `Coin`, `death_trap_tool` → `Death trap`), its `description`, and its kind (`bitmap` — click to place a sprite; `polygon` — click vertices, Enter / double-click / right-click to finish; `tool` — a mode).
- A tool of kind `tool` whose name is neither `select_tool` nor `delete_tool` is not implemented by the editor (it currently falls through to Select); its entry ends with **"— not yet implemented"**. This is derived from the same rule the editor uses to map tool names to modes, not from a hand-maintained list.
- If the toolbox layout failed to load, the section shows a single line saying the toolbox could not be loaded, rather than being empty.

## File menu section

| Item | Effect |
| --- | --- |
| Background Image | Choose a PNG to show behind the level. |
| Load Level | Open a level JSON file. |
| Save Level | Write the level JSON file. |
| Exit | Quit; asks whether to save first when there are unsaved changes. |

## Structure

- Content is built by a pure `help` module: `HelpContent::build(tools: Option<&[ToolDef]>, undo_label: &str, redo_label: &str) -> HelpContent` with `keyboard`, `mouse`, `tools`, `file` entry lists, plus `humanize_tool_name` and `is_tool_implemented` helpers, and `escape_closes_help(help_open) -> bool`. The module has no egui `Context` / `Ui` dependency so completeness is unit-tested.
- `main.rs` owns `help_open`, renders the window from `HelpContent`, and wires the menu item, `F1`, `?` and the Escape priority.

## Acceptance

- A1 — `Help → Keyboard & Commands` opens the help window.
- A2 — `F1` and `?` each toggle the help window.
- A3 — With help open, `Escape` closes it and does not cancel a polygon / edit mode; with help closed, `Escape` behaves as before.
- A4 — The window is closable, draggable and non-modal; arrow-key scrolling and canvas clicks keep working while it is open.
- A5 — Keyboard section lists Arrow keys, Enter, Escape, Delete/Backspace, Undo, Redo, F1 / ? and nothing the editor does not handle.
- A6 — Undo/Redo labels come from `format_shortcut` (⌘ on macOS, Ctrl elsewhere).
- A7 — Mouse section covers left click per tool, both double-click meanings, right-click, entity / vertex / toolbox drag, and click-outside-to-exit-edit.
- A8 — Tools section lists every `toolboxes.json` tool exactly once with description and kind; `add_point_tool` / `remove_point_tool` flagged "not yet implemented"; `select_tool` / `delete_tool` not flagged.
- A9 — File section lists the four menu items, noting Exit's unsaved-changes prompt.
- A10 — Body is in a bounded vertical `ScrollArea`; window fits at 800×720.
- A11 — Help state is view-only: not serialised, no effect on unsaved-changes.
