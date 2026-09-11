# Outcome

The level editor canvas scrolls in both axes with smooth, momentum-based motion, so a level larger than the window in width *and* height can be built and navigated using the arrow keys. Scrolling is no longer capped at the background image's edge: the canvas can be extended indefinitely to the right and downward while a level is being built.

# Scope

- Vertical scrolling via `ArrowUp` / `ArrowDown`, alongside the existing horizontal `ArrowLeft` / `ArrowRight`.
- Unbounded positive extent: the canvas scrolls past the background image's right and bottom edges and past all entity extents, so new entities can be placed in that space. Scrolling is clamped at the level origin (0, 0).
- Smooth motion: the editor repaints continuously while scrolling instead of relying on OS key-repeat events; a held key accelerates to a maximum speed; releasing it leaves momentum that decays to a full stop within a bounded time.
- Background rendering follows the vertical offset, and background tiles are bounded in both dimensions so tall images cannot exceed GPU texture limits.
- Background and entities are offset by the same rounded pixel value each frame, eliminating the relative shimmer caused today by the background truncating to whole pixels while entities move fractionally.
- Every existing consumer of the scroll offset (entity drawing, mouse→world conversion, selection hit-testing, minimap viewport rectangle, debug export) operates correctly on both axes.

# Non-goals

- Trackpad / mouse-wheel scrolling — the user chose arrow keys only.
- Drag-to-pan (space-drag or middle-button drag).
- Zooming.
- Any change to the level JSON format (`LevelData`), including how level size is persisted; `background_size` semantics are unchanged.
- Minimap redesign — it already computes both axes and is only exercised, not changed, beyond what the new offset requires.
- Upgrading `eframe` / `egui` / `winit`.
- Fixing the pre-existing compiler warnings unrelated to scrolling.

# Acceptance examples

- A1 — Vertical scroll: with the app running and a level loaded, holding `ArrowDown` moves the background and all entities upward on screen (`scroll_offset.y` increases); holding `ArrowUp` moves them back down.
- A2 — Horizontal scroll preserved: holding `ArrowRight` / `ArrowLeft` still scrolls horizontally exactly as before (`scroll_offset.x` increases / decreases).
- A3 — Unbounded positive extent: with the 25600×720 `level_image.png` loaded, holding `ArrowDown` scrolls past `y = 720` and `ArrowRight` past `x = 25600`; the canvas keeps scrolling into empty space and a bitmap placed there receives a world position beyond those extents. With *no* background loaded, scrolling still works in all four directions.
- A4 — Origin clamp: `scroll_offset.x` and `scroll_offset.y` are never negative. Momentum travelling toward the origin stops exactly at 0 rather than overshooting or bouncing.
- A5 — Continuous repaint: while any arrow key is held *or* scroll velocity is non-zero, the editor requests a repaint every frame, so motion does not depend on OS key-repeat timing.
- A6 — Momentum: releasing an arrow key does not stop the canvas immediately; velocity decays and reaches exactly zero within 1.5 seconds of release. There is no perpetual drift.
- A7 — Pixel alignment: the background destination rectangle and every entity's screen position are derived from the same rounded integer-pixel offset in a given frame, so background and entities never diverge by a fractional pixel.
- A8 — Background follows vertical scroll and tiles are bounded: the background draws offset by `−scroll_offset.y`, tiles never exceed 8192 px in either dimension, and an image taller than 8192 px is split into multiple tile rows (verifiable via the tile-grid computation without a GPU).
- A9 — Mouse→world on both axes: after scrolling vertically and horizontally, clicking at screen position `P` to place a bitmap or polygon vertex yields world position `P + scroll_offset` on both axes, and selecting an existing entity at its on-screen location hits it.
- A10 — Minimap viewport: the yellow viewport rectangle on the minimap moves down when scrolling down and right when scrolling right, reflecting `scroll_offset` on both axes.

# Constraints and invariants

- Level coordinates are anchored at the top-left origin, Y-down (egui convention); the Bevy integration flips Y against `background_height`. Negative coordinates therefore have no meaning in-game, which is why the origin clamp exists.
- `scroll_offset.x ≥ 0` and `scroll_offset.y ≥ 0` at all times, including mid-momentum.
- No upper bound on `scroll_offset` on either axis.
- Background tiles are ≤ 8192 px wide and ≤ 8192 px tall; at most the tiles intersecting the viewport plus a one-tile margin are resident.
- Background and entity rendering use one shared rounded offset per frame (`scroll_offset.round()`), never mixed truncation and float.
- `request_repaint()` is issued whenever scrolling input is held or velocity is non-zero; when the canvas is at rest the editor returns to reactive repainting.
- The scroll/momentum model is a pure, egui-free struct so it can be unit-tested deterministically with explicit `dt`.
- Level file format (`LevelData`, `LevelEntity`) is unchanged.
- Existing test `tests/level1_test.rs` keeps passing; `cargo build` keeps succeeding; no new dependencies.
- The `[profile.dev.package.objc2] debug-assertions = false` startup fix in `Cargo.toml` stays in place.

# Decisions

- D1 — Workspace: current directory (`--isolation current`). Reason: the editor's entire current state (all of `src/`, the `Cargo.toml` objc2 fix) is uncommitted; a worktree from `c52478c` would lack it and the app would not launch.
- D2 — Scroll input: arrow keys only, all four directions. Trackpad/wheel and drag-to-pan are explicitly out of scope (user choice).
- D3 — Level bounds: no upper bound — the user wants to "extend as much as I want during level buildup". Scrolling is clamped at the origin (0, 0) because level coordinates are anchored top-left and the Bevy converter flips Y against `background_height`, so negative positions are not meaningful. *(Origin clamp is inferred from the coordinate convention, not stated by the user — surfaced in CONFIRM.)*
- D4 — Motion feel: momentum / inertia (user choice). A held key accelerates to a max speed; release leaves a decaying velocity that stops within 1.5 s.
- D5 — (Agent) Pixel-alignment fix: background and entities share one rounded offset per frame. Chosen because the current `u32` truncation in `background.rs` versus float offsets in `entities.rs` is a concrete, observable cause of the reported roughness.
- D6 — (Agent) Root cause of jerkiness is the absence of `request_repaint()`; egui only redraws on input events, so held-key scrolling advances only at OS key-repeat rate. Fix: request a repaint every frame while scrolling.
- D7 — (Agent) The scroll model is implemented as a pure struct (velocity, position, clamp, decay) exercised via unit tests, so the smoothness and clamping behaviour is verifiable without a GPU or interactive session.

# Open questions

None. Shared understanding (scope, D1–D7, A1–A10, non-goals, and the inferred origin clamp) was explicitly confirmed by the user on 2026-09-11.

# Verification expectations

- `cargo build` succeeds (pre-existing warnings are acceptable; no new errors).
- `cargo test` passes, including the existing `tests/level1_test.rs`.
- New unit tests for the pure scroll model cover: each of the four directions (A1, A2); no upper clamp — position exceeds any given level size when scrolled far enough (A3); origin clamp including a momentum approach that stops at exactly 0 (A4); after release, velocity decays to exactly 0 within 1.5 s of simulated `dt` steps and never re-accelerates (A6); rounded offset is a single integer-valued `Vec2` (A7).
- New unit tests for the background tile grid cover: a tile never exceeds 8192 px in either dimension and an image taller than 8192 px yields more than one tile row (A8).
- Code inspection by the Verifier: `request_repaint` is called whenever an arrow key is held or velocity is non-zero (A5); `BackgroundImageController::draw` receives a `Vec2` offset and applies its `y` component (A8); mouse→world conversion and hit-testing add the full `Vec2` offset (A9); the minimap receives the same `scroll_offset` (A10).
- Optional smoke check: the app launches without aborting and stays alive (as verified earlier in this session for the objc2 fix); interactive scrolling itself cannot be asserted by a read-only Verifier and is covered by the unit tests plus inspection above.
