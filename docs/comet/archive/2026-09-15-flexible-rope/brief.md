# Outcome

Ropes look and move like ropes: a flexible chain that bends, dangles and trails, not a rigid rod. The part from the anchor to the character's hands stays taut and drives the swing exactly as today; the tail below the hands hangs from the hands and lags the swing; a free rope ripples and settles under gravity instead of swinging as one stick. The game mirrors this in a follow-up change.

# Scope

- The play simulation models every rope as a chain of short segments (a position-based / Verlet chain pinned at the anchor) advanced on the fixed step.
- While held, the chain from the anchor to the hands is pinned onto the straight taut line at the pendulum's angle and the hand link sits at the hold position; the rest of the chain hangs freely from the hands. The pendulum (grab, pump, climb, release, regrab lock, constants) is unchanged.
- The grab test and the hold arc length are measured along the actual chain, so a rope whose tail has swung away can be grabbed where it really is.
- On release the chain continues from its current shape and motion (no snap back to a straight line); a free rope is only ever the chain under gravity with damping, and settles to hanging straight.
- Drawing: in play the rope is drawn as a polyline through the chain links; while editing it stays a straight line from anchor to end.
- New constants in `game_config.rs` beside the other `ROPE_*` values; README wording says the rope is flexible.

# Non-goals

- Changing how ropes are placed, edited or saved (anchor + length, `rope_tool` bitmap entry).
- The rope colliding with level geometry (the chain passes through walls; only the character collides, as today).
- A fully physical rope where the character's swing emerges from the chain (the user chose the taut-segment model).
- Rope art or textures.
- The game side, which is a separate change on top of the `comet/rope-swing` branch.

# Acceptance examples

- A1 — A 300 px rope is a chain of `ROPE_SEGMENT_LEN` 16 px links (19 segments); after any number of steps every segment's length is within 2% of its rest length and the first link is exactly at the anchor.
- A2 — A free rope at rest hangs straight down. Displaced so its tail is 120 px to the side and released, it swings and ripples, then settles: within 6 s every link's speed is below `ROPE_SETTLE_SPEED` 2 px/s and the chain hangs straight again (all links within 1 px of the anchor's x).
- A3 — While held at hold length L, the links from the anchor to the hand lie on the straight line `anchor + s·dir(θ)` (within 0.5 px) and the hand link is at the hold position; the links below the hand hang from it.
- A4 — During a swing the tail lags: with the hand moving toward +x, the end link's x is behind the hand's x relative to the anchor's line (measured at the bottom of the swing).
- A5 — Grab, pump, climb, release and regrab behave exactly as before: the archived rope-swing tests (grab height, period, cap, climb bounds, release velocity plus hop, regrab lock, momentum carry-in, wall stop, hurt drop, respawn reset) still pass unchanged.
- A6 — The grab test follows the chain: a rope at rest is grabbed exactly as before; a rope whose tail has been displaced 60 px to the side can be grabbed at the displaced tail (which is where it is drawn) and not at the empty straight line.
- A7 — On release the chain keeps moving continuously: on the release step no link jumps by more than one segment length, and the tail keeps its velocity.
- A8 — In play the rope is drawn as a polyline through every link; while editing it is the straight anchor-to-end line; restart and stop put every rope back to hanging straight.
- A9 — Determinism: two simulations fed the same inputs produce identical chains.
- A10 — `game_config.rs` declares `ROPE_SEGMENT_LEN` 16 px, `ROPE_CHAIN_DAMPING` 0.985 per step, `ROPE_CHAIN_ITERATIONS` 80 and `ROPE_SETTLE_SPEED` 2 px/s; the README calls the rope flexible; `cargo test` and `cargo build` pass.

# Constraints and invariants

- The archived `rope-swing` capability defines the pendulum: one rigid pendulum from the anchor to the hold point, its `ROPE_*` constants, and the grab / climb / release rules confirmed by the user on 2026-09-14. This change keeps all of it and replaces only what the rope itself is between and beyond those points.
- The simulation is a pure module advanced on a fixed 1/120 s step; the chain must stay deterministic and unit-testable there.
- Rope chain state is play state only; nothing about it is serialised.

# Decisions

- D1 (Q1) — Flexibility is a **bending chain with the same swing**: the anchor-to-hands part is taut and drives the pendulum as today; the tail and any free rope are a simulated chain. Confirmed 2026-09-14.
- D2 — Chain model: position-based Verlet links of `ROPE_SEGMENT_LEN` 16 px (at least 3 per rope), link 0 pinned at the anchor, gravity `GRAVITY`, per-step velocity damping `ROPE_CHAIN_DAMPING` 0.985, `ROPE_CHAIN_ITERATIONS` 80 distance-constraint passes per step. While held, links from the anchor to the hand are pinned on the taut line and the hand link at the hold position.
- D3 — The hold arc length and the grab test are measured along the chain polyline (from `ROPE_MIN_HOLD` of arc length to the end); at grab the pendulum angle is the direction from the anchor to the grabbed link, and the taut part snaps straight (tension).
- D4 — On release, the link velocities are made continuous with the pendulum (each taut link keeps `s·ω` along the tangent) and the chain runs free from there; the rigid free-pendulum of the archived spec is gone.
- D5 — Settling: a free rope whose links are all slower than `ROPE_SETTLE_SPEED` snaps to hanging straight and rests until a grab or release disturbs it, so an idle rope is exactly vertical and still.
- D6 — The chain does not collide with geometry (non-goal).

# Open questions

None. Shared understanding confirmed by the user on 2026-09-14.

# Verification expectations

- `cargo test` (chain tests: segment lengths, anchor pin, settle, taut part, tail lag, chain grab, release continuity, determinism; all archived rope tests unchanged) and `cargo build` warning-free for touched code.
