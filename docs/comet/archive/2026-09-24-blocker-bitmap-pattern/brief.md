# Outcome

A blocker can be filled with a repeating pattern taken from an image file, instead of
showing only its outline. You select a blocker, pick a `.png`/`.jpg` from disk, and the
shape fills with that image tiled at its own pixel size and clipped to the outline. The
choice is saved in the level file and comes back when the level is loaded.

A `Patterns` window keeps the last twelve patterns you used as thumbnails. Once a texture
is in it, dressing the next blocker is one click instead of a trip through the file
dialog, and the list survives restarting the editor.

Today every polygon — blocker, wall or plain — is drawn as an outline and vertex dots
only (`PolygonEntity::draw`, `src/entities.rs:213-232`). The existing `color` field tints
that outline; nothing is ever filled. So this adds filling as well as patterning, and a
blocker becomes something you can recognise at a glance in a crowded level.

# Scope

## The pattern itself

- `PolygonEntity` gains `pattern: Option<String>`, the path to an image file.
- `LevelEntity::Polygon` gains a matching `#[serde(default)] pattern: Option<String>`, so
  existing level files load unchanged, and both conversions in `src/entities.rs` carry it.
- The editor caches pattern textures by path, loaded on demand, using `TextureOptions`
  with `wrap_mode: TextureWrapMode::Repeat`.
- A patterned polygon is drawn as a triangulated `Shape::Mesh` beneath its existing
  outline, with UVs placing one texture pixel on one world pixel so the image repeats at
  native size. The outline and vertex dots are unchanged and still drawn on top.
- Two new items in the `Level` menu: `Blocker Pattern…` opens the file dialog and applies
  the choice to the selected polygon, and `Clear Pattern` removes it. Both are disabled,
  with an explanation on hover, when no polygon is selected or while play mode is running.
- Setting or clearing a pattern goes through `save_state()`, so it is undoable.

## The gallery

- A floating `Patterns` window, opened from the `Level` menu, following the existing
  `Keyboard & Commands` window: `egui::Window` with `.open(&mut open)`, its rectangle
  remembered each frame.
- It shows the twelve most recently used patterns as image thumbnails, most recent first,
  plus a `Browse…` button that opens the same file dialog.
- Clicking a thumbnail applies that pattern to the selected polygon, through the same
  undoable path as the menu item. With no polygon selected the thumbnails are disabled and
  say why on hover, and the window still opens so you can see what is remembered.
- The list is stored in the existing `rust_game_editor_config.json` beside
  `last_background` and `last_level`, so it survives restarting the editor.
- Choosing a pattern already in the list moves it to the front rather than duplicating it;
  the thirteenth distinct pattern evicts the oldest.
- A remembered path whose file has since disappeared is skipped when the window is drawn
  and dropped from the list, so the gallery cannot accumulate dead entries.

# Non-goals

- The Bevy game is not changed. The editor writes the field; making the game render it is
  separate work, and `LEVEL_FORMAT.md`'s consumer example reads only `vertices` today.
- Polygons without a pattern keep their current appearance exactly: outline and dots, no
  fill. This change introduces no flat-colour fill.
- No per-polygon control over the pattern's scale, offset, rotation or tint. It tiles at
  native size from the polygon's bounding-box origin, and that is the whole vocabulary.
- The gallery is a recency list, not a managed asset library: no naming, tagging,
  reordering, pinning or manual removal of entries.
- Bitmap entities, ropes and the background are untouched.
- The image is referenced by path, not copied into or embedded in the level file.
- No change to collision, to `polygon_type`, or to how the game hulls polygons.

# Acceptance examples

- Scenario: With a blocker selected, choosing `Level → Blocker Pattern…` and picking
  `/tmp/bricks.png` stores `Some("/tmp/bricks.png")` as that polygon's pattern and leaves
  every other polygon's pattern `None`.
- Scenario: A polygon carrying a pattern is saved and loaded again, and the pattern path
  survives the round trip unchanged.
- Scenario: A level file whose polygon entries have no `pattern` key at all loads without
  error, and those polygons come back with `pattern: None`.
- Scenario: A polygon with no pattern is drawn exactly as before — outline segments and
  vertex dots, and no filled mesh is emitted for it.
- Scenario: A 64x64 pattern on a polygon whose bounding box is 256 wide and 128 tall
  produces texture coordinates spanning 4.0 across and 2.0 down, so the image repeats four
  times horizontally and twice vertically at its native size.
- Scenario: A concave blocker shaped like an L is filled only inside its outline: the
  triangulation produces `n - 2` triangles for an `n`-vertex polygon and their combined
  area equals the polygon's own area.
- Scenario: Setting a pattern pushes an undo entry, and undo restores the polygon to
  having no pattern.
- Scenario: `Clear Pattern` on a polygon that has one returns its pattern to `None`, and
  that is undoable too.
- Scenario: Setting a pattern marks the level as having unsaved changes.
- Scenario: When no polygon is selected, or while play mode is running, the guard that
  drives the pattern menu items and the gallery thumbnails reports that they are
  unavailable.
- Scenario: A polygon whose pattern path does not exist or cannot be decoded is drawn as a
  plain outline, exactly like a polygon with no pattern, and the editor does not panic.
- Scenario: Using three different patterns in turn leaves the gallery holding all three,
  most recently used first.
- Scenario: Using a pattern that is already the third entry in the gallery moves it to the
  front and leaves the list twelve long at most, with no duplicate entry.
- Scenario: Using a thirteenth distinct pattern evicts the oldest, so the list holds
  exactly twelve.
- Scenario: The gallery list is written to `rust_game_editor_config.json` and read back on
  the next launch, leaving `last_background` and `last_level` in that file untouched.
- Scenario: A remembered pattern whose file no longer exists is dropped from the gallery
  rather than shown as a broken thumbnail.
- Scenario: Clicking a gallery thumbnail with a blocker selected applies that pattern to
  it, through the same undoable path as the menu item.

# Constraints and invariants

- The editor's world/screen mapping is `world = screen + scroll_offset`; the fill must use
  the same mapping as the outline so the two never drift apart when scrolling.
- Tiling follows the precedent already set by the background: `BackgroundImageController`
  draws a repeated image with `painter.image` over a computed visible range
  (`src/background.rs:151-205`).
- The `Patterns` window's rectangle must join `help_rect` and `size_dialog_rect` in
  `pointer_over_panels` (`src/main.rs:630`), or a click on a thumbnail would also place a
  polygon point on the canvas behind the window.
- `new_level::level_hash` hashes `format!("{entities:?}")`, so a new field on
  `PolygonEntity` joins unsaved-change tracking automatically; no change is needed there,
  but a test must prove it.
- The gallery is editor preference state, not level data: it must never affect
  `has_unsaved_changes`, the level hash, or what is written to a level file.
- Writing the config must preserve keys the editor does not own, exactly as the existing
  last-path writers do; reading a config with no gallery key must yield an empty gallery.
- Adding the field must not alter the JSON written for polygons that have no pattern
  beyond the new key itself, and must not break `tests/level1_test.rs` or `level1.json`.
- Pattern textures must be cached by path rather than reloaded per frame, and thumbnails
  must reuse that same cache rather than decoding images again.
- Drawing must stay allocation-light per frame and must never panic on a degenerate
  polygon (fewer than three points, zero-area, or repeated vertices).

# Decisions

- **The pattern is any image file chosen from disk.** Confirmed by the user over the
  bundled bitmaps and over cropping the background. It mirrors how Background Image
  already works, and the path is stored as a string exactly as `LevelData::background` is.
- **The image tiles at its native pixel size**, confirmed by the user over stretching one
  copy to the bounding box, so a small texture covers a large blocker without distortion.
- **The pattern is saved into the level JSON**, confirmed by the user, as a new optional
  field defaulting to absent. The user was told the Bevy game will not honour it without
  its own change and accepted that.
- **A gallery of recently used patterns**, added to the scope by the user after the first
  Shape was prepared.
- **The gallery is a floating window of thumbnails**, confirmed by the user over a
  submenu of file names and over a strip inside the toolbox, because seeing a texture is
  the point of remembering it.
- **The gallery holds twelve, most recent first, persisted across sessions** in the
  editor's existing config file, confirmed by the user over a session-only list and over
  deriving the list from the open level.
- **The field lives on `PolygonEntity`, not only on blockers.** Blockers are polygons, and
  special-casing one `polygon_type` in the data model would be arbitrary; the menu simply
  acts on whichever polygon is selected.
- **The fill follows the drawn outline, including concave shapes**, using ear-clipping
  triangulation rather than a convex hull, so what is filled is what is drawn. The game
  hulls polygons for collision, but that is a separate concern from how the editor draws.
- **The affordance is `Level` menu items**, following `Level → Level Size…`, disabled
  with a hover explanation in the same way `File → New Level` is disabled during play.
- **Isolation: current directory**, on `main`, matching all ten previous Native changes.

# Open questions

None. Every decision affecting user-visible behaviour was confirmed before Build.

# Verification expectations

- `cargo build` succeeds with no new warnings beyond the existing baseline.
- `cargo test` passes, including new tests for: the set/clear mapping, the save/load round
  trip, loading a file with no `pattern` key, the UV tiling arithmetic, the triangulation
  of a concave polygon, undo, the unsaved-changes hash, the menu guard, gallery ordering,
  deduplication, eviction at twelve, config round trip, and pruning of missing files.
- `tests/level1_test.rs` continues to pass against the existing `level1.json`.
- A test proves the gallery does not affect `has_unsaved_changes` or the level hash.
- Every pre-existing test continues to pass unchanged.
