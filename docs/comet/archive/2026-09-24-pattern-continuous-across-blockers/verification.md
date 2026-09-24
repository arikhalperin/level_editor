---
generated_from_state_version: 12
---

# Verification

## Current result

- Result: **Archived**
- Verification status: **Checks completed; result confirmed**
- Goal cycle: 1
- Iteration: 2
- Verifier attempt: 1
- Completed: 2026-09-24T14:45:16.199Z
- Summary: Independent read-only Verifier assessed iteration 2 and passed 6/6, judging the change ready to archive. It re-ran cargo build (10 warnings proper, none in pattern.rs, confirming bounding_box was deleted outright rather than left dead, and the Rect import removed with it) and cargo test (245 plus 1 integration, 0 failed). It confirmed both iteration-1 defects genuinely closed: the rewritten assertion message is structurally guaranteed true by uv_for's signature and the assertion is the sole guard of scroll independence, and the origin parameter removal is complete at every call site with its guard now exercised, the negative-size case doing the real work by producing a finite wrong mirror coordinate rather than an obvious infinity. It diffed the test names against HEAD, confirming exactly one authorised removal, thirteen survivors, one strengthened rewrite and four additions. It found one new defect, a tautological assertion in a test I added, which my handoff had oversold.

## Acceptance

| ID | Result | Source | Criterion | Reason |
| --- | --- | --- | --- | --- |
| A1 | passed | brief.md | Scenario: Two blockers share the edge x = 256, one spanning world x 0..256 and the other x 256..512, both with a 64x64 pattern. The texture coordinate at the shared edge is 4.0 computed from either blocker, so the pattern runs across the join unbroken. | two_blockers_sharing_an_edge_continue_the_pattern_across_it (pattern.rs:349-373): left spans 0..256, right 256..512, texture 64x64. Asserts left_uv[1].x == 4.0, right_uv[0] == left_uv[1], right_uv[1].x == 8.0. The verifier confirmed the two sides come from two independent fill_mesh calls via the uvs helper, so the agreement assertion is genuinely cross-shape and not a self-comparison. |
| A2 | passed | brief.md | Scenario: A blocker whose bounding box starts at world (1000, 500), with a 64x64 pattern, has texture coordinate (15.625, 7.8125) at that corner rather than (0, 0), because the tiling is measured from the world origin. | the_tiling_is_measured_from_the_world_not_the_shapes_own_corner (pattern.rs:336-348) asserts uv[0] == (1000/64, 500/64) = (15.625, 7.8125), both exactly representable in f32 so the equality is exact rather than lucky, plus an explicit assert_ne against (0,0), which is what shape anchoring would have given. |
| A3 | passed | brief.md | Scenario: A blocker whose bounding box already starts at the world origin is drawn exactly as it was before this change, with its corner at texture coordinate (0, 0) and a 256x128 shape spanning 4.0 by 2.0. | pattern.rs:325-334 asserts corner (0,0) and uv[2] == (4.0, 2.0) for the 256x128 square, now routed through fill_mesh rather than raw uv_for, which the verifier judged strictly stronger than the HEAD version. It cannot discriminate this change, since a shape cornered on the origin answers the same either way, and that is stated in the test's own comment and in the disclosed limits rather than oversold. |
| A4 | passed | brief.md | Scenario: Moving a blocker changes its texture coordinates, so the pattern stays fixed to the world and the shape slides over it. This is the accepted cost of neighbours lining up, and is asserted so it cannot change unnoticed. | moving_a_blocker_slides_it_over_a_pattern_fixed_to_the_world (pattern.rs:374-387) asserts before[0] != after[0] and the exact value after[0] == (0.5, 0.0) for a 32px move over a 64px tile. The exact-value assertion is what makes the accepted trade-off impossible to change unnoticed. |
| A5 | passed | brief.md | Scenario: The fill still follows the outline: a concave blocker yields `n - 2` triangles whose combined area equals the polygon's own area, unchanged by the new anchor. | a_concave_shape_is_filled_only_inside_its_outline (pattern.rs:389-414) still yields n-2 triangles whose area is within 0.01 of the polygon's own 6000, with every centroid inside by ray casting. triangulate has zero delta in this change, so the anchor cannot affect it. |
| A6 | passed | brief.md | Scenario: A polygon with no pattern still emits no fill, and a pattern whose image will not decode still falls back to a bare outline. | an_unpatterned_polygon_is_drawn_with_no_fill_at_all (main.rs:3030-3038) and a_pattern_whose_image_will_not_load_falls_back_to_the_bare_outline (main.rs:3040-3051). pattern_fill still short-circuits on the missing pattern and on a cached None texture before reaching fill_mesh; unchanged by this diff. |

## Checks

| Check | Command | Working directory | Status | Exit | Duration |
| --- | --- | --- | --- | ---: | ---: |
| cargo build | build | . | passed | 0 | 555 ms |
| cargo test | test | . | passed | 0 | 3703 ms |

### Builder-reported evidence

These are Builder reports, not Runtime check receipts or independent verification results.

- cargo build: passed — 12 warning lines, unchanged baseline, none in pattern.rs. Note for the record: the previous verifier pointed out this figure counts cargo's two trailing summary lines, so it is 10 warnings proper.
- cargo test: passed — 245 unit tests plus 1 integration, 0 failed.
- mutation check: passed — From the previous candidate and still applicable: restoring bounding-box anchoring fails the world-anchoring, shared-edge and moving tests. The regression-guard test correctly still passes, since its shape is cornered on the world origin.
- Known limitation: The trade-off is unchanged and confirmed in Shape: a pattern is fixed to the world, so dragging a blocker slides the shape over it. Pinned by an exact-value assertion.
- Known limitation: Still no executable coverage of draw_polygon_pattern, the Mesh packing or TextureWrapMode::Repeat, so nothing automated proves two adjacent blockers visually line up on screen; the texture coordinates they would be drawn with are asserted directly, which is the strongest headless evidence available.
- Known limitation: A3 remains a pure regression guard that cannot discriminate this change, since a shape cornered on the world origin gets the same answer under either anchoring. That is stated in the test's own comment, and A1, A2 and A4 carry the discriminating power.
- Known limitation: This does nothing about an image that does not tile cleanly against itself: any internal joins in the image remain, now running continuously across blockers rather than restarting at each one. That is the separate seamless-texture problem.
- Known limitation: Everything carried over from the previous change is unchanged: per-frame re-triangulation with an O(n^3) worst case, pattern_textures never evicted, exists() unable to distinguish a deleted file from an unmounted volume, permanent per-session failure caching, the untested egui paths, and the cosmetic items recorded there.

## Blockers

_None._

## Risks and skipped work

- D1, a defect in my own test work that the verifier was right to call out: a_world_point_maps_to_the_same_coordinate_whichever_polygon_asks (pattern.rs:318-322) asserts uv_for(x, t) == uv_for(x, t), calling a deterministic pure function twice with identical arguments. It is tautological, cannot fail for any implementation that does not return NaN, and proves nothing about polygons since uv_for no longer takes a polygon or an origin. Only its first assertion is load-bearing. My handoff summary claimed it stated the property the change rests on directly, which oversells it: that property is actually carried by the two-blockers test, which compares two independent fill_mesh results. Harmless and passing, but it reads as coverage it does not provide and should be deleted or the test renamed to what its first line proves.
- Scroll independence is pinned by exactly one test, fill_mesh_pairs_every_vertex_with_its_texture_coordinate (pattern.rs:437-453), the only fill_mesh call anywhere with a non-zero scroll. The verifier confirmed it is a real guard by computing what the assertions would become if the UV leaked the scroll, and that the rewritten message is structurally guaranteed true by uv_for's signature.
- The zero-size condition is now duplicated, at pattern.rs:37 in uv_for and pattern.rs:150 in fill_mesh. Defensible for a public function, but worth knowing if the threshold ever changes.
- Pos2::ZERO as the sentinel for a zero-size texture is an arbitrary choice that would read as the texture's top-left to an external caller. fill_mesh rejects the identical condition first and no other caller exists, so no live path reaches it.
- Pre-existing and out of scope: texture_size.x <= 0.0 is false for NaN, so a NaN texture size yields NaN coordinates from both guards. Identical to HEAD.
- A3 remains a non-discriminating regression guard by construction; A1, A2 and A4 carry the discriminating power, which the verifier confirmed by reasoning out that restoring bounding-box anchoring would fail all three while A3 correctly still passes.
- Unchanged and carried over: no executable coverage of draw_polygon_pattern, the Mesh packing or TextureWrapMode::Repeat, so nothing automated proves two adjacent blockers visually line up on screen; per-frame re-triangulation with an O(n^3) worst case; pattern_textures never evicted; and the image's own internal joins are untouched, now running continuously across blockers rather than restarting at each one.

## Previous iterations

| Goal cycle | Iteration | Attempt | Outcome | Unresolved | Summary | Completed |
| ---: | ---: | ---: | --- | --- | --- | --- |
| 1 | 1 | 1 | recovery | — | Verifier passed 6/6 and confirmed the anchoring is world / texture_size with no residual shape dependence and provably scroll-independent. Two minor defects to fix before archive: a stale assertion message at pattern.rs:423 still claims the texture is anchored to the shape, which is now false and sits in the very test guarding scroll-independence; and uv_for's origin parameter is vestigial now that the only production caller passes Pos2::ZERO and no test calls it directly, leaving its guard unreachable and untested. | 2026-09-24T14:40:48.437Z |
| 1 | 2 | 1 | pass | — | Independent read-only Verifier assessed iteration 2 and passed 6/6, judging the change ready to archive. It re-ran cargo build (10 warnings proper, none in pattern.rs, confirming bounding_box was deleted outright rather than left dead, and the Rect import removed with it) and cargo test (245 plus 1 integration, 0 failed). It confirmed both iteration-1 defects genuinely closed: the rewritten assertion message is structurally guaranteed true by uv_for's signature and the assertion is the sole guard of scroll independence, and the origin parameter removal is complete at every call site with its guard now exercised, the negative-size case doing the real work by producing a finite wrong mirror coordinate rather than an obvious infinity. It diffed the test names against HEAD, confirming exactly one authorised removal, thirteen survivors, one strengthened rewrite and four additions. It found one new defect, a tautological assertion in a test I added, which my handoff had oversold. | 2026-09-24T14:45:16.199Z |



## Conclusion

Independent read-only Verifier assessed iteration 2 and passed 6/6, judging the change ready to archive. It re-ran cargo build (10 warnings proper, none in pattern.rs, confirming bounding_box was deleted outright rather than left dead, and the Rect import removed with it) and cargo test (245 plus 1 integration, 0 failed). It confirmed both iteration-1 defects genuinely closed: the rewritten assertion message is structurally guaranteed true by uv_for's signature and the assertion is the sole guard of scroll independence, and the origin parameter removal is complete at every call site with its guard now exercised, the negative-size case doing the real work by producing a finite wrong mirror coordinate rather than an obvious infinity. It diffed the test names against HEAD, confirming exactly one authorised removal, thirteen survivors, one strengthened rewrite and four additions. It found one new defect, a tautological assertion in a test I added, which my handoff had oversold.
