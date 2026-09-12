//! Pure help-screen content: what every key, mouse gesture, tool and menu item does.
//!
//! This module has no dependency on egui's `Context` or `Ui`, so the claim that the
//! help documents every command the editor handles is checked by unit tests.
//! `main.rs` renders `HelpContent` and owns the open/closed state.

use crate::toolbox::ToolDef;

/// One row of the help window: what to press / click on the left, its effect on the right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    pub label: String,
    pub effect: String,
}

impl HelpEntry {
    fn new(label: impl Into<String>, effect: impl Into<String>) -> Self {
        Self { label: label.into(), effect: effect.into() }
    }
}

/// The four sections of the help window, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpContent {
    pub keyboard: Vec<HelpEntry>,
    pub mouse: Vec<HelpEntry>,
    pub tools: Vec<HelpEntry>,
    pub file: Vec<HelpEntry>,
}

/// Suffix appended to a tool the editor does not implement yet.
pub const NOT_IMPLEMENTED: &str = "not yet implemented";
/// Shown in the Tools section when `src/toolboxes.json` failed to load.
pub const NO_TOOLBOX: &str = "Toolbox could not be loaded (src/toolboxes.json).";

/// What one press of Escape does. Closing the help always wins so a stray Escape can
/// never also throw away a half-drawn polygon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    CloseHelp,
    CancelPolygon,
    ExitEditMode,
    Nothing,
}

/// Whether Escape is claimed by the help window. Checked before any other Escape
/// meaning so a stray press can never also discard a half-drawn polygon.
pub fn escape_closes_help(help_open: bool) -> bool {
    help_open
}

pub fn escape_action(help_open: bool, drawing_polygon: bool, editing_polygon: bool) -> EscapeAction {
    if escape_closes_help(help_open) {
        EscapeAction::CloseHelp
    } else if drawing_polygon {
        EscapeAction::CancelPolygon
    } else if editing_polygon {
        EscapeAction::ExitEditMode
    } else {
        EscapeAction::Nothing
    }
}

/// `coin_tool` -> `Coin`, `death_trap_tool` -> `Death trap`.
pub fn humanize_tool_name(name: &str) -> String {
    let base = name.strip_suffix("_tool").unwrap_or(name);
    let words = base.replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Mirrors the tool-name match in `EditorState::update`: only `select_tool` and
/// `delete_tool` are real modes; any other `tool`-kind entry falls through to Select.
pub fn is_tool_implemented(def: &ToolDef) -> bool {
    def.tool_type != "tool" || matches!(def.name.as_str(), "select_tool" | "delete_tool")
}

fn kind_hint(tool_type: &str) -> &'static str {
    match tool_type {
        "bitmap" => "click to place a sprite",
        "polygon" => "click vertices; Enter, double-click or right-click to finish",
        "tool" => "mode",
        _ => "unknown kind",
    }
}

impl HelpContent {
    /// `undo_label` / `redo_label` come from `egui::Context::format_shortcut` so the
    /// caller decides between ⌘ and Ctrl; this module stays platform-agnostic.
    pub fn build(tools: Option<&[ToolDef]>, undo_label: &str, redo_label: &str) -> Self {
        let keyboard = vec![
            HelpEntry::new("← → ↑ ↓", "Scroll the canvas. Hold to accelerate; release to glide to a stop."),
            HelpEntry::new("Enter", "Finish the polygon being drawn (needs at least 3 vertices)."),
            HelpEntry::new(
                "Escape",
                "Close this help; otherwise cancel the polygon being drawn or leave polygon edit mode.",
            ),
            HelpEntry::new(
                "Delete / Backspace",
                "Remove the selected vertex of the polygon being edited (a polygon keeps at least 3).",
            ),
            HelpEntry::new(undo_label, "Undo the last change."),
            HelpEntry::new(redo_label, "Redo the last undone change."),
            HelpEntry::new("F1 / ?", "Show or hide this help."),
            HelpEntry::new("F5", "Start or stop play mode."),
            HelpEntry::new("R", "Respawn the character at the spawn point (play mode)."),
            HelpEntry::new(
                "A / D  or  \u{2190} / \u{2192}",
                "Move the character left and right (play mode).",
            ),
            HelpEntry::new(
                "W / S  or  \u{2191} / \u{2193}",
                "Look up and down; down also fast-falls and climbs down (play mode).",
            ),
            HelpEntry::new("Space or Z", "Jump, and wall jump off a wall (play mode)."),
            HelpEntry::new("Shift, K or C", "Dash (play mode)."),
        ];

        let mouse = vec![
            HelpEntry::new(
                "Left click (play mode)",
                "Move the spawn point and respawn there. The level is never changed while playing.",
            ),
            HelpEntry::new(
                "Left click — Select tool",
                "Select the entity under the cursor; drag to move it. Click empty canvas to deselect.",
            ),
            HelpEntry::new("Left click — Delete tool", "Remove the entity under the cursor."),
            HelpEntry::new("Left click — polygon tools", "Add a vertex at the cursor."),
            HelpEntry::new(
                "Left click — bitmap tools",
                "Place the sprite at the cursor (a translucent preview follows the cursor).",
            ),
            HelpEntry::new(
                "Double-click — polygon tool",
                "Close and finish the polygon (needs at least 3 vertices).",
            ),
            HelpEntry::new(
                "Double-click — Select tool on a polygon",
                "Enter vertex-edit mode for that polygon.",
            ),
            HelpEntry::new("Right click — polygon tool", "Finish the polygon being drawn."),
            HelpEntry::new(
                "Drag (edit mode)",
                "Move the selected vertex; click a vertex first to select it.",
            ),
            HelpEntry::new("Click outside the polygon (edit mode)", "Leave vertex-edit mode."),
            HelpEntry::new("Drag the toolbox", "Move the toolbox anywhere on screen."),
        ];

        let tools = match tools {
            Some(defs) if !defs.is_empty() => defs
                .iter()
                .map(|def| {
                    let mut effect = format!("{} ({})", def.description, kind_hint(&def.tool_type));
                    if !is_tool_implemented(def) {
                        effect.push_str(" — ");
                        effect.push_str(NOT_IMPLEMENTED);
                    }
                    HelpEntry::new(humanize_tool_name(&def.name), effect)
                })
                .collect(),
            _ => vec![HelpEntry::new("Tools", NO_TOOLBOX)],
        };

        let file = vec![
            HelpEntry::new("File \u{2192} Background Image", "Choose a PNG to show behind the level."),
            HelpEntry::new("File \u{2192} Load Level", "Open a level JSON file."),
            HelpEntry::new("File \u{2192} Save Level", "Write the level JSON file."),
            HelpEntry::new(
                "File \u{2192} Exit",
                "Quit; asks whether to save first when there are unsaved changes.",
            ),
            HelpEntry::new(
                "Level \u{2192} Level Size\u{2026}",
                "Set the level's width and height in pixels; saved with the level.",
            ),
            HelpEntry::new("Play \u{2192} Play / Stop", "Start or stop play mode (same as F5)."),
            HelpEntry::new("Help \u{2192} Keyboard & Commands", "Show this window."),
        ];

        Self { keyboard, mouse, tools, file }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, tool_type: &str, description: &str) -> ToolDef {
        ToolDef {
            name: name.to_string(),
            description: description.to_string(),
            tool_type: tool_type.to_string(),
            icon: format!("assets/{name}.png"),
            color: None,
        }
    }

    fn build_default(tools: Option<&[ToolDef]>) -> HelpContent {
        HelpContent::build(tools, "⌘Z", "⇧⌘Z")
    }

    fn labels(entries: &[HelpEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.label.as_str()).collect()
    }

    fn all_text(entries: &[HelpEntry]) -> String {
        entries.iter().map(|e| format!("{} {}\n", e.label, e.effect)).collect()
    }

    #[test]
    fn escape_closes_help_first_regardless_of_polygon_state() {
        assert_eq!(escape_action(true, true, true), EscapeAction::CloseHelp);
        assert_eq!(escape_action(true, true, false), EscapeAction::CloseHelp);
        assert_eq!(escape_action(true, false, true), EscapeAction::CloseHelp);
        assert_eq!(escape_action(true, false, false), EscapeAction::CloseHelp);
    }

    #[test]
    fn escape_closes_help_predicate_matches_the_dispatch() {
        assert!(escape_closes_help(true));
        assert!(!escape_closes_help(false));
        for drawing in [true, false] {
            for editing in [true, false] {
                assert_eq!(
                    escape_closes_help(true),
                    escape_action(true, drawing, editing) == EscapeAction::CloseHelp
                );
            }
        }
    }

    #[test]
    fn escape_keeps_previous_meaning_when_help_is_closed() {
        assert_eq!(escape_action(false, true, true), EscapeAction::CancelPolygon);
        assert_eq!(escape_action(false, true, false), EscapeAction::CancelPolygon);
        assert_eq!(escape_action(false, false, true), EscapeAction::ExitEditMode);
        assert_eq!(escape_action(false, false, false), EscapeAction::Nothing);
    }

    #[test]
    fn humanizes_tool_names() {
        assert_eq!(humanize_tool_name("coin_tool"), "Coin");
        assert_eq!(humanize_tool_name("death_trap_tool"), "Death trap");
        assert_eq!(humanize_tool_name("select_tool"), "Select");
        assert_eq!(humanize_tool_name("polygon"), "Polygon");
        assert_eq!(humanize_tool_name(""), "");
    }

    #[test]
    fn keyboard_section_lists_every_handled_key_and_nothing_else() {
        let c = build_default(None);
        let labels = labels(&c.keyboard);
        assert_eq!(
            labels,
            vec![
                "← → ↑ ↓",
                "Enter",
                "Escape",
                "Delete / Backspace",
                "⌘Z",
                "⇧⌘Z",
                "F1 / ?",
                "F5",
                "R",
                "A / D  or  ← / →",
                "W / S  or  ↑ / ↓",
                "Space or Z",
                "Shift, K or C",
            ],
            "keyboard section must match the keys handled in EditorState::update exactly"
        );
    }

    #[test]
    fn undo_and_redo_labels_come_from_the_caller() {
        let c = HelpContent::build(None, "Ctrl+Z", "Shift+Ctrl+Z");
        assert!(labels(&c.keyboard).contains(&"Ctrl+Z"));
        assert!(labels(&c.keyboard).contains(&"Shift+Ctrl+Z"));
        assert!(!all_text(&c.keyboard).contains('⌘'), "no hard-coded mac symbols");
    }

    #[test]
    fn mouse_section_covers_every_gesture() {
        let text = all_text(&build_default(None).mouse);
        for needle in [
            "Left click (play mode)",
            "Left click — Select tool",
            "Left click — Delete tool",
            "Left click — polygon tools",
            "Left click — bitmap tools",
            "Double-click — polygon tool",
            "Double-click — Select tool on a polygon",
            "Right click — polygon tool",
            "Drag (edit mode)",
            "Click outside the polygon (edit mode)",
            "Drag the toolbox",
        ] {
            assert!(text.contains(needle), "mouse section missing: {needle}");
        }
    }

    #[test]
    fn menu_section_lists_every_menu_item_and_the_exit_prompt() {
        let c = build_default(None);
        assert_eq!(
            labels(&c.file),
            vec![
                "File → Background Image",
                "File → Load Level",
                "File → Save Level",
                "File → Exit",
                "Level → Level Size…",
                "Play → Play / Stop",
                "Help → Keyboard & Commands",
            ],
            "the menus section must list every menu item the editor offers"
        );
        assert!(c.file[3].effect.contains("unsaved"), "Exit must mention the unsaved-changes prompt");
    }

    #[test]
    fn tools_section_has_one_entry_per_tool_with_description_and_kind() {
        let defs = [
            def("coin_tool", "bitmap", "Tool to add a coin on the map."),
            def("wall_tool", "polygon", "Tool to add a platform on the map."),
            def("select_tool", "tool", "Tool to select objects on the map."),
        ];
        let c = build_default(Some(&defs));
        assert_eq!(labels(&c.tools), vec!["Coin", "Wall", "Select"]);
        assert!(c.tools[0].effect.contains("Tool to add a coin on the map."));
        assert!(c.tools[0].effect.contains("click to place a sprite"));
        assert!(c.tools[1].effect.contains("click vertices"));
        assert!(c.tools[2].effect.contains("mode"));
    }

    #[test]
    fn unimplemented_point_tools_are_flagged_and_real_modes_are_not() {
        let defs = [
            def("add_point_tool", "tool", "Tool to add a point of interest on the map."),
            def("remove_point_tool", "tool", "Tool to remove a point of interest from the map."),
            def("select_tool", "tool", "Tool to select objects on the map."),
            def("delete_tool", "tool", "Tool to delete objects from the map."),
            def("orc_tool", "bitmap", "Tool to add an orc character on the map."),
        ];
        assert!(!is_tool_implemented(&defs[0]));
        assert!(!is_tool_implemented(&defs[1]));
        assert!(is_tool_implemented(&defs[2]));
        assert!(is_tool_implemented(&defs[3]));
        assert!(is_tool_implemented(&defs[4]));

        let c = build_default(Some(&defs));
        let flagged: Vec<&str> = c
            .tools
            .iter()
            .filter(|e| e.effect.contains(NOT_IMPLEMENTED))
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(flagged, vec!["Add point", "Remove point"]);
    }

    #[test]
    fn real_toolboxes_json_yields_every_tool_exactly_once() {
        // cargo runs tests from the package root, where src/toolboxes.json lives.
        let layout = crate::toolbox::load_toolbox_layout().expect("src/toolboxes.json must parse");
        let c = build_default(Some(&layout.tools));
        assert_eq!(c.tools.len(), layout.tools.len());
        let mut seen = std::collections::HashSet::new();
        for (entry, def) in c.tools.iter().zip(&layout.tools) {
            assert_eq!(entry.label, humanize_tool_name(&def.name));
            assert!(entry.effect.contains(&def.description), "{}: description missing", def.name);
            assert!(seen.insert(entry.label.clone()), "duplicate tool entry {}", entry.label);
            assert_eq!(
                entry.effect.contains(NOT_IMPLEMENTED),
                !is_tool_implemented(def),
                "{}: implemented flag mismatch",
                def.name
            );
        }
        let flagged: Vec<&str> = c.tools.iter().filter(|e| e.effect.contains(NOT_IMPLEMENTED)).map(|e| e.label.as_str()).collect();
        assert_eq!(flagged, vec!["Add point", "Remove point"]);
    }

    #[test]
    fn missing_or_empty_toolbox_shows_a_notice_instead_of_nothing() {
        let c = build_default(None);
        assert_eq!(c.tools.len(), 1);
        assert_eq!(c.tools[0].effect, NO_TOOLBOX);
        let c = build_default(Some(&[]));
        assert_eq!(c.tools.len(), 1);
        assert_eq!(c.tools[0].effect, NO_TOOLBOX);
    }
}
