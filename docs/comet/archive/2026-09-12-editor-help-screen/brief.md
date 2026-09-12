# Outcome

The level editor gains an in-app help screen that documents every command and key the editor responds to: keyboard shortcuts, mouse interactions per tool, every toolbox tool, and the File menu. It opens from a new **Help** menu or from **F1** / **?**, and closes with **Escape** or the window's close button.

# Scope

- A new `Help` menu in the top menu bar (to the right of `File`) with one item, `Keyboard & Commands`, that opens the help window.
- `F1` and `?` (Shift+/) toggle the help window from anywhere in the editor.
- The help window is a floating, draggable, closable, **non-modal** `egui::Window` (same family as the existing "Unsaved Changes" dialog). Its content is vertically scrollable so it stays usable at the editor's minimum window size (800×720).
- `Escape` closes the help window when it is open, and that key press does nothing else. When the window is closed, `Escape` keeps its existing meaning (cancel polygon drawing / exit polygon edit mode).
- Content is grouped into four sections:
  1. **Keyboard** — every handled key: Arrow keys (scroll; hold to accelerate; momentum on release), Enter (finish polygon), Escape (close help / cancel polygon / exit polygon edit), Delete or Backspace (remove selected polygon vertex, polygons keep ≥ 3), Undo, Redo, and F1 / ? (this help). Modifier names are platform-aware via `ctx.format_shortcut` (⌘ on macOS, Ctrl elsewhere).
  2. **Mouse** — left click per current tool (Select: select an entity and drag to move it; Delete: remove the clicked entity; polygon tools: add a vertex; bitmap tools: place the sprite at the cursor); double-click (polygon tool: close the polygon; Select on a polygon: enter vertex-edit mode); right-click (finish the polygon being drawn); drag (move a selected entity; move a vertex while editing; move the toolbox by dragging it); click outside a polygon while editing it to exit edit mode.
  3. **Tools** — generated at runtime from `src/toolboxes.json`: each tool's humanised name, its description, and its kind (bitmap / polygon / tool). Tools of kind `tool` that the editor does not implement (`add_point_tool`, `remove_point_tool` — they currently fall through to Select) are marked "not yet implemented".
  4. **File menu** — Background Image, Load Level, Save Level, Exit (prompts to save when there are unsaved changes).
- Help-window state is view state only: it is never written to the level file and never marks the level as having unsaved changes.
- The help content builder is a pure, egui-free module so its completeness is unit-testable.

# Non-goals

- Updating `README.md` (its shortcut list is stale — says arrows scroll left/right only). Left for a separate change; can be offered after this one.
- Implementing `add_point_tool` / `remove_point_tool`.
- Rebindable shortcuts, a settings screen, or tooltips on toolbox icons.
- Localisation.
- A modal or "first launch" help; the window only appears on request.
- Changing any existing shortcut's behaviour other than Escape's new first priority while help is open.

# Acceptance examples

- A1 — Help menu: the menu bar shows `Help`; choosing `Help → Keyboard & Commands` opens the help window.
- A2 — Key toggle: pressing `F1` opens the help window when closed and closes it when open; pressing `?` does the same.
- A3 — Escape priority: with the help window open and a polygon in progress (or a polygon in edit mode), one press of `Escape` closes the help window and the polygon / edit mode is untouched; with the help window closed, `Escape` cancels the polygon / exits edit mode exactly as before.
- A4 — Non-modal window: the help window has a title-bar close button, can be dragged, and while it is open the arrow keys still scroll the canvas and clicks outside the window still reach the canvas and toolbox.
- A5 — Keyboard section completeness: the Keyboard section lists entries for Arrow keys, Enter, Escape, Delete/Backspace, Undo, Redo, and F1 / ? — every key the input handler responds to — and nothing that the editor does not handle.
- A6 — Platform modifiers: on macOS the Undo entry reads with ⌘ (e.g. "⌘Z") and Redo with ⇧⌘Z; on other platforms they read "Ctrl+Z" / "Shift+Ctrl+Z". Rendered via `egui::Context::format_shortcut`, not hard-coded strings.
- A7 — Mouse section completeness: the Mouse section covers left click per tool (Select, Delete, polygon tools, bitmap tools), double-click (close polygon; enter vertex edit), right-click (finish polygon), drag (entity, vertex, toolbox), and click-outside-to-exit-edit.
- A8 — Tools generated from JSON: every tool in `src/toolboxes.json` appears exactly once with its description and kind; `add_point_tool` and `remove_point_tool` carry a "not yet implemented" marker; `select_tool` and `delete_tool` do not. Adding a tool to the JSON makes it appear without a code change.
- A9 — File menu section lists Background Image, Load Level, Save Level, and Exit, noting that Exit prompts when there are unsaved changes.
- A10 — Fits the window: the help content is inside a vertical `ScrollArea` with a bounded max height so at 800×720 the window is fully on screen and its content can be scrolled to the end.
- A11 — View state only: opening/closing help never changes `has_unsaved_changes()` and nothing about help is serialised into `LevelData`.

# Constraints and invariants

- `egui` 0.27.2 provides `Key::F1`, `Key::Questionmark`, and `Context::format_shortcut`; no dependency changes.
- Existing shortcuts and mouse behaviour are unchanged except that `Escape` first closes an open help window.
- `F1` / `?` toggling is gated off while a text field wants keyboard input (`ctx.wants_keyboard_input()`), consistent with the arrow-key gating.
- Tool section source of truth is `ToolboxLayout.tools` (already loaded at startup); if the layout failed to load, the Tools section says so instead of being empty.
- "Not yet implemented" is derived, not hand-listed: a tool of kind `tool` whose name is neither `select_tool` nor `delete_tool` is unimplemented, mirroring the match in `update` that maps tool names to `Tool` variants.
- No changes to `LevelData`, `ScrollModel`, or `BackgroundImageController`.
- `cargo build` and `cargo test` continue to pass; the existing 21 tests are untouched.

# Decisions

- D1 — Workspace: current directory (`--isolation current`). The only uncommitted file is an unrelated `README.md` edit, which this change does not touch; a worktree would gain nothing.
- D2 — Open/close: `Help` menu item **and** `F1` **and** `?`; close via Escape or the title-bar button (user choice).
- D3 — Presentation: floating, draggable, non-modal `egui::Window` with a scrollable body (user choice).
- D4 — Tool descriptions generated from `toolboxes.json`, with unimplemented tools flagged (user choice).
- D5 — (Agent) Escape gets a strict priority: close help first, consume the press. Rationale: the alternative — Escape closing help *and* cancelling a half-drawn polygon — would destroy work the user did not intend to discard.
- D6 — (Agent) The window is non-modal so the user can read the help while trying the commands; this follows from the "floating window" choice.
- D7 — (Agent) Content lives in a pure `help` module (`HelpContent` built from `&[ToolDef]` + `is_mac`) with unit tests asserting completeness, so the "documents every key" claim is verifiable without a GUI.
- D8 — (Agent) Modifier rendering uses `ctx.format_shortcut` so macOS shows ⌘ / ⇧ and other platforms show Ctrl / Shift.

# Open questions

None. Shared understanding (scope, D1–D8, A1–A11, non-goals, current-directory workspace) was explicitly confirmed by the user on 2026-09-12.

# Verification expectations

- `cargo build` succeeds with no new warnings in the new module; `cargo test` passes including new unit tests.
- Unit tests on the pure help module: every `ToolDef` passed in yields exactly one entry with its description (A8); `add_point_tool` / `remove_point_tool` flagged and `select_tool` / `delete_tool` not (A8); Keyboard entries include Arrow, Enter, Escape, Delete/Backspace, Undo, Redo, F1/? (A5); Mouse entries mention left click, double-click, right-click, drag, toolbox, edit-mode exit (A7); File entries are the four menu items with Exit's prompt (A9); the Escape-priority function returns "close help" when help is open regardless of polygon state and falls through otherwise (A3); an empty tool list yields the "no toolbox loaded" notice.
- Verifier code inspection in `main.rs`: `Help` menu button present (A1); `F1` / `Questionmark` toggle gated by `wants_keyboard_input` (A2); Escape branch checks `help_open` first and returns before polygon handling (A3); `egui::Window` with `.open(&mut …)`, default resizable/draggable, body in `ScrollArea::vertical().max_height(…)` (A4, A10); modifiers rendered through `ctx.format_shortcut` (A6); no help field in `LevelData` and `has_unsaved_changes` unaffected (A11).
- Smoke run: app launches and stays alive; no panic.
