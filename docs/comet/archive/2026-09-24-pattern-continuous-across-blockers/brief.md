# Outcome

Two blockers that touch read as one continuous surface. The stonework runs straight
across their shared edge instead of breaking at it.

Today each polygon anchors its tiling to its own bounding box
(`fill_mesh`, `src/pattern.rs:169-173`), so every blocker restarts the pattern from its
own top-left corner. Put two beside each other and the pattern jumps at the seam,
however carefully they were drawn. Anchoring instead to the world origin gives every
polygon one shared tiling grid, and neighbours line up by construction.

# Scope

- `fill_mesh` computes texture coordinates from the world origin rather than the
  polygon's bounding box. One line, plus removing the now-unused bounds lookup.
- `bounding_box` loses its only caller. It moves into the test module if the tests still
  want it, or goes entirely; it must not be left behind as dead code.
- The two tests that pin bounding-box anchoring are replaced by tests that pin world
  anchoring, including one that asserts two adjacent shapes agree along a shared edge.

# Non-goals

- No change to the level format. This is how an existing field is drawn, not new data.
- No per-polygon control over the tiling offset, phase or origin. One grid for the whole
  level is the whole point; a per-shape nudge would reintroduce exactly the seam this
  removes.
- No change to triangulation, clipping, the outline, the gallery, or anything about which
  image is chosen.
- The pattern is not made seamless within itself. If the image does not tile cleanly, its
  own internal joins stay visible; that is a property of the image, and a separate piece
  of work.
- Bitmap entities, ropes and the background are untouched.

# Acceptance examples

- Scenario: Two blockers share the edge x = 256, one spanning world x 0..256 and the
  other x 256..512, both with a 64x64 pattern. The texture coordinate at the shared edge
  is 4.0 computed from either blocker, so the pattern runs across the join unbroken.
- Scenario: A blocker whose bounding box starts at world (1000, 500), with a 64x64
  pattern, has texture coordinate (15.625, 7.8125) at that corner rather than (0, 0),
  because the tiling is measured from the world origin.
- Scenario: A blocker whose bounding box already starts at the world origin is drawn
  exactly as it was before this change, with its corner at texture coordinate (0, 0) and
  a 256x128 shape spanning 4.0 by 2.0.
- Scenario: Moving a blocker changes its texture coordinates, so the pattern stays fixed
  to the world and the shape slides over it. This is the accepted cost of neighbours
  lining up, and is asserted so it cannot change unnoticed.
- Scenario: The fill still follows the outline: a concave blocker yields `n - 2`
  triangles whose combined area equals the polygon's own area, unchanged by the new
  anchor.
- Scenario: A polygon with no pattern still emits no fill, and a pattern whose image will
  not decode still falls back to a bare outline.

# Constraints and invariants

- Screen positions still come from `world - scroll_offset`; only the texture coordinate
  changes. Scrolling the view must not shift the pattern relative to the level.
- The tiling must not depend on the order polygons are drawn in, or on which polygon is
  selected.
- `cargo build` must gain no warning, which means `bounding_box` cannot simply be left
  uncalled.
- Every existing test must still pass except the two that encode bounding-box anchoring,
  which are replaced deliberately.

# Decisions

- **Anchor to the world origin.** Confirmed by the user, who reported the break between
  adjacent blockers as the thing that looks wrong.
- **The pattern stays fixed to the world when a shape moves.** This follows from world
  anchoring and is the one behaviour that gets worse: previously a pattern travelled with
  its polygon. It is recorded as an acceptance example rather than hidden, because it is
  the direct cost of the outcome the user asked for.
- **No AI model is needed for this.** The user offered one; the break is an anchoring
  choice, not a synthesis problem, so a model would add weight without addressing it.
  Making a non-seamless image tile cleanly against itself is the separate problem where a
  model would earn its place.
- **Isolation: current directory**, on `main`, matching all eleven previous changes.

# Open questions

None. The one decision affecting user-visible behaviour was confirmed before Build.

# Verification expectations

- `cargo build` succeeds with no new warnings beyond the existing baseline.
- `cargo test` passes, including a new test asserting two adjacent shapes agree at their
  shared edge, and one pinning the world-fixed behaviour when a shape moves.
- Every pre-existing test passes unchanged except the two replaced deliberately.
