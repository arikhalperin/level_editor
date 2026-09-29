# Outcome

A pattern stops showing its joins. Where `cave_stone.png` currently tiles with a hard
edge every 126 pixels across and 104 down, a blocker reads as one continuous stone
surface.

The joins are not subtle and not a matter of taste. Measured against the texture's own
variation, its wrap seams are a mean absolute RGB difference of 68.4 left-to-right and
97.2 top-to-bottom, where a step between neighbouring columns averages 9.3 and between
neighbouring rows 10.7. Every tile boundary is a discontinuity seven to nine times
sharper than anything occurring naturally inside the image.

There is a second artefact behind the first: 126x104 is small, so the same recognisable
patch repeats across a large blocker. Removing the seams alone would leave that grid
legible.

# Scope

- A new `quilt` module implementing Efros-Freeman image quilting: lay patches from the
  source on an output grid with overlap, choose each patch by best match over its
  overlap against what is already placed, and join along a minimum-error cut through the
  overlap rather than a blend, so detail stays crisp.
- The output is toroidal: patches in the last column and row are also matched against
  the opposite edge, so the synthesised tile wraps against itself and the fill has no
  seam anywhere.
- A `seam_ratio` measurement: the mean absolute RGB difference between opposite edges,
  divided by the mean difference between neighbouring lines. A pattern is quilted only
  when that ratio exceeds a threshold, so a texture that already tiles cleanly is used
  untouched rather than needlessly resynthesised.
- Quilting runs when a pattern is applied, producing a 512x512 tile.
- The result is cached on disk beside the editor's config, keyed by the source path, its
  modification time and the synthesis parameters, so a pattern is quilted once and
  reused on later launches.
- Synthesis is deterministic: the random choices are driven by a seed derived from that
  same key, so the same image always yields the same tile and a level looks identical
  every time it is opened.
- The Patterns window shows the tile that will actually be drawn, so what you see in the
  gallery is what a blocker will look like.

# Non-goals

- The user's image file is never modified. The quilted tile is a cached derivative; the
  level keeps referencing the source path exactly as it does now.
- No change to the level format, to how a pattern is chosen, to the gallery's recency
  rules, or to world-anchored tiling.
- No neural model and no new dependency. This is an algorithm over pixels the `image`
  crate already decodes.
- No user-facing controls for patch size, overlap, output size or the seam threshold.
  One good default, chosen here, not a panel of sliders.
- Quilting is not applied to the background image, to bitmap entities, or to anything
  but polygon patterns.
- No attempt to preserve global structure. Quilting is right for organic textures like
  stone; a strongly structured pattern such as regular brick courses can have its
  alignment broken by it, and that is a known cost rather than something this fixes.

# Acceptance examples

- Scenario: A source whose opposite edges differ far more than its interior does, such as
  the measured 7.4 across and 9.0 down of `cave_stone.png`, is reported by `seam_ratio`
  as needing work, and a synthetic image whose edges match its interior is reported as
  not needing it.
- Scenario: Quilting `cave_stone.png` produces a 512x512 tile whose own wrap seams are
  no worse than its interior variation, so the ratio that was 7.4 and 9.0 falls to about
  1 in both directions.
- Scenario: Every pixel of a quilted tile comes from the source image: the synthesis
  copies and cuts, it never invents colours, so no output pixel is a colour absent from
  the source.
- Scenario: Quilting the same source twice with the same key yields byte-identical
  output, so a level does not change appearance between launches.
- Scenario: Quilting two different sources, or the same source under a different key,
  yields different output, so the determinism is a seeded choice rather than a fixed
  pattern of patches.
- Scenario: A minimum-error cut through an overlap region follows the path of least
  difference: for an overlap contrived so one column matches far better than the others,
  the cut runs down that column.
- Scenario: A source smaller than one patch, or one pixel in size, is returned unchanged
  rather than crashing or producing an empty tile.
- Scenario: A pattern whose quilted tile is already cached is not quilted again on a
  later launch; the cached tile is loaded instead.
- Scenario: A pattern that does not need quilting is drawn from its source image, and no
  cache entry is written for it.
- Scenario: A source image that cannot be decoded still falls back to a bare outline,
  exactly as now, and the editor does not panic.

# Constraints and invariants

- The rendering path does not change. Quilting produces an image; the existing
  `pattern_texture`, `fill_mesh` and `draw_polygon_pattern` continue to work on whatever
  image they are given, still with `TextureWrapMode::Repeat` and world-anchored
  coordinates.
- Quilting must never run per frame. It happens once per pattern, and the cache is
  consulted before any synthesis.
- The synthesis must be pure and testable without an egui context or a real file: the
  algorithm takes pixels and returns pixels.
- Determinism must not depend on `HashMap` iteration order or on any address, clock or
  thread identity.
- A failure to quilt, to read the cache or to write it must degrade to using the source
  image, never to losing the pattern or panicking.
- `cargo build` must gain no warning beyond the existing baseline, and every existing
  test must continue to pass.

# Decisions

- **Quilt only when the measurement says it is needed**, confirmed by the user over
  always quilting and over an explicit action. An already-seamless texture is left
  exactly alone, since resynthesis could only lose fidelity.
- **512x512 output**, confirmed by the user over 256 and 1024: roughly four times the
  source area, enough that the repeat stops reading as a grid, at about 1MB per pattern
  and a bake well under a second.
- **A cached derivative, never a rewrite of the user's file.** The level keeps pointing
  at the source image, so nothing the user owns is altered and the cache can be deleted
  at any time without consequence.
- **Deterministic by a seed derived from the cache key**, so the same level looks the
  same on every machine and every launch, which a wall-clock or random seed would not
  give.
- **The gallery shows the quilted tile**, because the gallery exists to show what a
  blocker will look like.
- **Isolation: current directory**, on `main`, matching all twelve previous changes.

# Open questions

None. Both decisions affecting user-visible behaviour were confirmed before Build.

# Verification expectations

- `cargo build` succeeds with no new warnings beyond the existing baseline.
- `cargo test` passes, including tests for the seam measurement on both a seamy and a
  clean image, the fall in seam ratio after quilting, the copy-not-invent property,
  determinism under the same key, difference under different keys, the minimum-error cut,
  and degenerate sources.
- A test proves a cached tile is reused rather than resynthesised.
- Every pre-existing test continues to pass unchanged.
