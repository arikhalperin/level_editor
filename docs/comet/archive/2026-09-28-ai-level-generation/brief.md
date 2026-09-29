# Outcome

The editor can generate a complete Hollow-Knight-style level on its own, driven by an AI
model. The user opens a dialog, describes the area they want in plain words, and presses
Generate. The editor asks the model to author the level, proves the result is actually
playable by driving the editor's own play simulation from the start of the area to its end,
and puts the finished level on the canvas, where it can be edited, play-tested and saved
exactly like a hand-built one.

By default the model is OpenAI's, reached over its API with a key from the environment. A
local model server is still supported — it is the same wire protocol — by pointing the
endpoint back at localhost.

# Scope

## Reaching the model

- The editor talks to any **OpenAI-compatible HTTP API** (`POST {endpoint}/chat/completions`).
  The endpoint URL and model name are settings, defaulting to `https://api.openai.com/v1` and
  `gpt-6-astra`. They are remembered in the editor's existing config alongside the other
  remembered paths. Pointing the endpoint at `http://localhost:11434/v1` still drives a local
  model server, unchanged.
- **`https://` is now supported**, which it deliberately was not before: reaching OpenAI
  requires TLS.
- The API key is read from the **`OPENAI_API_KEY` environment variable** and sent as
  `Authorization: Bearer`. The editor never writes it anywhere, never logs it, and never
  shows it: the dialog reports only whether a key was found. With no key set and an endpoint
  that needs one, Generate fails before any request, naming the variable.
- Requests ask for JSON-schema-constrained output where the server supports it, and fall
  back to lenient parsing (first balanced JSON object in the reply) where it does not.
- All network work happens on a worker thread. The UI thread never blocks: while a
  generation is running the editor keeps repainting, shows its progress, and can cancel it.
- When no server answers, the endpoint is wrong, the model name is unknown, or the reply is
  unusable after its retries, generation fails with a message naming the endpoint and the
  actual cause. The open level is left exactly as it was.

## What the model produces

- **The model authors the level JSON itself**, including every coordinate. The editor does
  not expand a high-level plan into geometry.
- The editor asks in stages: first an area outline (the chambers, their rectangles, how they
  connect, which is the start and which is the end, and each chamber's intended difficulty),
  then each chamber's entities in its own request. The model still decides all geometry;
  staging bounds each reply's length, keeps each one inside the shape the schema asks for,
  and lets progress be reported.
- **The outline must name a connection point for every pair of adjoining chambers**, and each
  chamber request states the entry and exit points that chamber must provide floor at. Filling
  chambers in isolation was the flaw behind levels that always needed repairing: a chamber
  whose floor sat at a different height from its neighbour's left an unclimbable step at the
  boundary, so the route died there every time.
- The model may use only the editor's existing vocabulary: `wall_tool` (climbable) and
  `blocker_tool` polygons, plain polygons, and the `coin_tool`, `death_trap_tool`,
  `orc_tool` and `rope_tool` bitmaps. Anything else is rejected and re-asked.

## What a generated level is

- A connected multi-chamber area: several chambers joined by corridors and shafts, a
  critical path from the area's start to its end, and optional side branches with rewards.
- The area's extent is the current level size when one is set, otherwise a default; the
  generated level's `level_size` is set to the extent actually used.

## Proving it is playable

- Every generated level is **proven traversable by the editor's own play simulation**
  before it is accepted. The prover builds the same `World` play mode builds, then searches
  from the level's `spawn` to its `exit` by driving the real `Simulation` through motion
  primitives (run, jump at several hold lengths, dash, wall jump, wall climb, rope grab and
  release), keeping the input sequence that reaches the exit as evidence.
- Structural validation runs first and is cheap: known entity names only, polygons with at
  least three finite vertices inside the level bounds, positive sizes, and ropes shaped as
  `LEVEL_FORMAT.md` requires.

## When the proof fails

- The failure is fed back to the model in terms it can act on (which ledge the route dies
  on, how far the nearest foothold is, and what the character can actually clear), and the
  offending chamber is re-asked. This happens at most twice.
- If the route is still unproven, **deterministic repair** makes the smallest geometry edits
  that let the simulation through — lowering or extending a landing ledge, narrowing a gap,
  adding a `wall_tool` face to climb — re-proving after each edit within a bounded budget.
- If repair cannot finish within its budget, generation fails, says so plainly, and leaves
  the open level untouched. A level is never delivered unproven.
- The result panel says whether the level is the model's own geometry or was repaired, and
  where it was repaired.

## Where the result lands

- Generation replaces the current level, guarded by the editor's existing unsaved-changes
  confirmation and applied as a single undoable step, so one Ctrl+Z restores what was open.
- The menu item is disabled during play mode, like `New Level` and `Load Level`.

## Level format

- `LevelData` gains `spawn: Option<[f32; 2]>` and `exit: Option<[f32; 2]>`, both
  `#[serde(default)]`, exactly as `level_size` was added: every level file written before
  this change loads unchanged.
- When the open level carries a `spawn`, pressing Play starts the character there instead of
  at the centre of the visible canvas. A click during play still moves the spawn, and
  `New Level` still clears it.

# Non-goals

- Installing, downloading, bundling or managing a local model. If the endpoint is a local
  server, the user runs it; nothing is installed on their behalf.
- Any provider other than OpenAI and OpenAI-compatible servers. No Anthropic, Google or
  Azure-specific authentication or request shapes.
- Storing, managing, validating or displaying the API key. It comes from the environment or
  it does not come at all: no key field, no keychain, no config entry.
- Tracking, estimating, capping or reporting what a generation costs.
- Guaranteeing that a generated level is *good*. The guarantee is structural: valid entities
  and a route the simulation can actually walk. Pacing and beauty are the model's business.
- Generating art, tilesets, backgrounds, sprites or a pattern image.
- Editing an existing level by prompt ("make this harder"); generation always replaces.
- New entity types, new tools, or any level-format change beyond `spawn` and `exit`.
- Changing the Bevy game. It may adopt `spawn` and `exit` later; until then it ignores them.
- Changing rope, combat, camera, minimap or scrolling behaviour.
- Making the automated player a feature the user can drive. It is an internal prover, with
  no UI to watch or replay the route it found.

# Acceptance examples

- A1 — With no server listening on the configured endpoint, Generate fails within the
  connection timeout with a message naming the endpoint and the cause; the entities, level
  size, background and unsaved-changes state of the open level are all unchanged.
- A2 — The model client sends `POST {endpoint}/chat/completions` with the configured model
  name and, when a key is available, an `Authorization: Bearer` header carrying it; the
  endpoint and model settings survive a save and reload of the editor config.
- A3 — A reply wrapped in prose or a ```json fence still parses: the first balanced JSON
  object is extracted and used.
- A4 — A reply naming an entity outside the toolbox vocabulary (for example
  `bitmap_name: "spike_wall"`) is rejected, that chamber is re-asked, and no unknown name
  ever reaches the canvas.
- A5 — Structural validation rejects, and re-asks for, a polygon with fewer than three
  vertices, a non-finite or out-of-bounds coordinate, a non-positive bitmap size, and a
  `rope_tool` entry whose length is below 60.
- A6 — Generation runs on a worker thread: while a generation is in flight the editor keeps
  repainting and the progress panel reports the current stage; Cancel ends it and leaves the
  open level untouched.
- A7 — The traversal prover, given a hand-built level with a 200 px gap between two ledges
  at equal height, proves the route and returns an input sequence that the simulation
  replays to the exit.
- A8 — The prover, given the same level with the gap widened to 1400 px and no wall to climb
  and no rope, reports the route unprovable and names the ledge the route dies on.
- A9 — The prover proves a route that requires each advanced move in turn: a dash-only gap
  (between `JUMP_HEIGHT`-reach and `DASH_DISTANCE` 450), a wall-jump-only shaft, and a
  climb-only face of `wall_tool`.
- A10 — A model reply whose chamber fails the proof is re-asked at most twice, and the
  feedback sent back names the failing ledge and the distance to the nearest foothold.
- A11 — When retries are exhausted, deterministic repair edits the geometry until the
  simulation proves the route, and the delivered level's route is proven by the same prover
  that rejected the original.
- A12 — When repair cannot succeed within its budget, generation fails with that reason and
  the open level is untouched; no partially repaired level is ever delivered.
- A13 — Accepting a generated level replaces the current one in a single undoable step: one
  Ctrl+Z restores the previous entities, level size and spawn exactly.
- A14 — With unsaved changes, Generate opens the existing Save / Discard / Cancel
  confirmation; Cancel leaves the level and the generation request untouched.
- A15 — The `Generate` menu item is disabled while play mode is running, with the same
  hover explanation `New Level` uses.
- A16 — `LevelData` round-trips `spawn` and `exit`; a level file written before this change
  (no `spawn`, no `exit` keys) loads with both `None` and no error, and an explicit `null`
  for either loads as `None`.
- A17 — Pressing Play on a level whose `spawn` is (1200, 700) starts the character at
  (1200, 700) rather than the centre of the visible canvas; on a level with no `spawn` the
  view-centre behaviour is unchanged; a click during play still moves the spawn; and
  `New Level` clears it.
- A18 — An end-to-end generation driven by a recorded model transcript (no live server)
  produces a level whose every entity is in the toolbox vocabulary, whose `spawn` and `exit`
  are set, whose `level_size` matches the extent used, and whose route the prover confirms.
- A19 — The defaults are OpenAI's: a fresh config yields endpoint `https://api.openai.com/v1`
  and model `gpt-6-astra`, and `Generate` is available without the user typing a model name.
  An endpoint pointed at `http://localhost:11434/v1` is still accepted and still drives a
  local server over plain HTTP.
- A20 — An `https://` endpoint is accepted rather than refused, and a request to one is made
  over TLS. Offline this is asserted against a local TLS listener; the live case is A23.
- A21 — The API key is read from `OPENAI_API_KEY` and never persisted: after a generation, the
  editor's config file contains no value matching the key, and neither the key nor any
  `Authorization` header value appears in any log line the editor emits.
- A22 — With no `OPENAI_API_KEY` set and an endpoint that requires one, `Generate` fails before
  any request is sent, with a message naming the variable; the open level is untouched. With a
  localhost endpoint and no key, no key is required and no `Authorization` header is sent.
- A23 — A live smoke test against OpenAI, run with the user's own key: one generation produces
  a level whose route the prover confirms. This is the only acceptance item that leaves the
  machine and the only one that cannot be run offline; it must be recorded as actually run or
  as not run, never assumed.
- A24 — The outline request asks for a connection point between each pair of adjoining
  chambers, and each chamber request states the entry and exit points that chamber must put
  floor at. A recorded transcript whose chambers meet at their stated connection points
  produces a level the prover confirms **without any repair**, and the neighbouring floors at
  each connection point are within a jumpable step of one another.

# Constraints and invariants

- Established facts about this machine (investigated 2026-09-24):
  - Hardware is an Intel Core i7-8850H with 32 GB RAM on macOS 15.8, with no Metal or CUDA
    acceleration. This is why a local model was slow, and why OpenAI is now the default.
  - No local model runtime is installed, so the local path remains untested against a real
    local server.
  - Homebrew on this machine has no bottles for this tier and builds from source, which is
    relevant to any dependency with a C or assembly component.
- **Every automated acceptance item must be verifiable offline.** The model client sits behind
  a trait so the pipeline can be driven by recorded transcripts and stub replies, and HTTP is
  asserted against a local socket; the validator, prover and repairer are tested on hand-built
  levels. One acceptance item is a live smoke test against OpenAI, which needs the user's own
  key and is the only item that leaves the machine.
- **Prompts leave the machine.** With the default endpoint, the level description and the
  generation prompts are sent to OpenAI. This is inherent to the request and is stated in the
  README so it is not a surprise.
- The editor is an `eframe`/`egui` desktop app with no HTTP client dependency today; one is
  added. The UI thread must not block while the model is thinking.
- The level file format is otherwise fixed: `LevelData` with `bitmap` and `polygon` entities
  (`LEVEL_FORMAT.md`). Generated levels must be ordinary levels that load, save and edit
  exactly like hand-placed ones.
- The entity vocabulary is the toolbox in `src/toolboxes.json`: `wall_tool` (climbable
  platform polygon), `blocker_tool` polygon, plain `polygon_tool`, and the `coin_tool`,
  `death_trap_tool`, `orc_tool` and `rope_tool` bitmaps.
- `src/game_config.rs` holds the transcribed character kit that defines what is reachable:
  `JUMP_HEIGHT` 365, `PLAYER_RUN_SPEED` 600, `DASH_DISTANCE` 450 with one air dash,
  `WALL_JUMP_HEIGHT` 190 / `WALL_JUMP_HORIZONTAL_DISTANCE` 170, wall slide and
  `CLIMB_SPEED` 140 on `wall_tool` polygons, `POGO_HEIGHT` 160, `MAX_AIR_JUMPS` 0,
  `GRAVITY` 1400. The prover uses the simulation driven by these; it never re-derives them.
- `Simulation::new(spawn)` takes a spawn position and a `World` of collision polygons and
  ropes and advances on a fixed 1/120 s step from an `Input`, so it can be driven headlessly
  by a search. The prover must not change the simulation's behaviour: play mode and the
  prover must stay the same simulation, or the proof means nothing.
- The coordinate model is world pixels anchored top-left, Y-down.
- The archived `play-spawn-at-view-centre` change (2026-09-20) declared "No change to the
  level format. The spawn point is still not saved to the level JSON" as a non-goal. D7
  deliberately supersedes that non-goal; its view-centre behaviour is kept intact as the
  fallback for levels that carry no `spawn`.
- Generation must remain deterministic given the same model replies, so a recorded
  transcript reproduces a level exactly; a seed is passed to servers that honour one.
- **A9's third case cannot be met as literally worded, and is met in substance instead.**
  A "climb-only" geometry does not exist in this character's kit: any wall can be wall-slid
  on, and a wall jump gains `WALL_JUMP_HEIGHT` off a plain face as readily as off a
  `wall_tool` one, so a lone wall can be ratcheted up by jumping off it and returning to it
  higher. This was found by building the comparison A9 implies and watching the
  non-`wall_tool` control succeed. Comparing reachability with and without `wall_tool`
  therefore proves nothing about climbing. What is delivered instead, and is stronger than a
  label check: a route up a `wall_tool` face is proven and replays, and climbing is isolated
  directly — holding into a `wall_tool` face and pressing up gains height, while the same
  face without `wall_tool` gains none. The dash and wall-jump cases are met as worded, each
  with a negative control (the same gap widened past a dash; the same shaft with its faces
  removed).

# Decisions

- D12 (2026-09-25) — **OpenAI is the default model provider, and local support is kept.** The
  endpoint stays a free-text setting; only its default changes, to
  `https://api.openai.com/v1`. Pointing it at a localhost server still works, because it is
  the same wire protocol either way. Confirmed 2026-09-25, after the user reported a local
  model was too slow and needed repairing on every attempt.
- D13 (2026-09-25) — **The API key comes from `OPENAI_API_KEY` and nowhere else.** It is never
  written to the editor's config, never logged, and never displayed; the dialog reports only
  whether one was found. A key in a config file is a plaintext secret that backup and sync
  tools copy, and the editor has no business holding one. Confirmed 2026-09-25.
- D14 (2026-09-25) — **The default model is `gpt-6-astra`**, OpenAI's most capable general
  model, confirmed against OpenAI's own model documentation on 2026-09-25 rather than from
  memory. The field stays free text, so any model id works. The flagship tier was chosen
  because level design needs spatial reasoning and strictly-shaped JSON, and a level costs
  only a few thousand tokens. Confirmed 2026-09-25.
- D15 (2026-09-25) — **The outline carries connection points between adjoining chambers, and
  each chamber is told the entry and exit it must provide floor at.** This addresses the
  reported symptom directly: chambers were being filled in isolation, so neighbouring floors
  at different heights left an unclimbable step at the boundary and the route died there,
  which is why repair ran on every level. Agent decision within the confirmed scope, since a
  connected multi-chamber area was always the requirement (D2).
- D1 (Q2) — **The model writes the level JSON itself.** It authors the entities and their
  coordinates directly; the editor does not expand a high-level plan into geometry.
  Confirmed 2026-09-24. The user chose this over the plan-and-expand option after being
  shown that direct JSON emission costs minutes per level on this CPU and needs repair.
- D2 (Q3) — **One generated level is a connected multi-chamber area**: several chambers
  linked by corridors and shafts, a critical path from the start to the end of the area,
  optional side branches and rewards. Confirmed 2026-09-24.
- D3 (Q4) — **A generated level is proven traversable by the existing play simulation.**
  An automated player drives `src/sim.rs` and must actually reach the end of the critical
  path before the level is accepted; analytic reach checks alone are not sufficient.
  Confirmed 2026-09-24.
- D4 (Q5) — **Generation replaces the current level**, guarded by the existing
  unsaved-changes prompt and recorded as a single undoable step. Confirmed 2026-09-24.
- D5 (Q1) — The model runs **locally as a running server**; a per-call CLI binary is ruled
  out. Confirmed 2026-09-24.
- D6 (Q1a) — The wire protocol is a **configurable OpenAI-compatible endpoint**
  (default `http://localhost:11434/v1`), not Ollama's native API, so any local server works
  and the endpoint can later point at a faster machine without a code change.
  Confirmed 2026-09-24.
- D7 (Q7) — **`LevelData` gains optional `spawn` and `exit` fields**, added the same way
  `level_size` was, so old files load unchanged. Play mode honours `spawn` when present.
  Confirmed 2026-09-24.
- D8 (Q6) — **On a failed proof: retry, then repair.** The failure goes back to the model
  at most twice; if the route is still unprovable, deterministic repair makes the smallest
  geometry edits that let the simulation through; if that fails within budget, generation
  fails and the open level is untouched. Confirmed 2026-09-24.
- D9 — Requests are staged (area outline, then one request per chamber) rather than one
  giant reply. This is an implementation consequence of D1 on CPU-only inference: it bounds
  each reply's length and makes progress reportable, and the model still authors all
  geometry. Agent decision, no user-visible behaviour is traded away.
- D11 — **Superseded 2026-09-25.** It said no HTTP dependency would be added and that
  `https://` would be refused, because the only endpoint was a server on the user's own
  machine. Reaching OpenAI requires TLS, so a TLS-capable HTTP client is now a dependency and
  `https://` is supported. The hand-rolled socket client is replaced by it.
- D10 — Assessed for Supervisor decomposition and rejected: the model client, the prover and
  the repairer all meet in one generate-prove-repair pipeline and would be edited together,
  so coordination would cost more than it saves. This proceeds as one Native change.

# Open questions

None. Every outcome-affecting decision was confirmed by the user on 2026-09-24.

# Verification expectations

- `cargo test` and `cargo build` clean for touched code, per project convention.
- Offline for everything except A23: no automated acceptance item requires a live service. The
  end-to-end item (A18) runs from a recorded transcript, HTTP from a local socket, and TLS from
  a local TLS listener.
- A23 is a live check needing the user's key and their explicit go-ahead, since it sends their
  prompt to OpenAI and costs them money. It must be reported as run or not run, never assumed.
- Prover tests are the heart of it: hand-built levels for the jump, dash, wall-jump, climb
  and rope cases, plus the provably-impossible case, each asserting the verdict and, where a
  route exists, replaying the returned input sequence through the simulation to the exit.
- Format tests follow the `level_size` precedent already in `src/level_data.rs`: round trip,
  missing keys, explicit nulls.
