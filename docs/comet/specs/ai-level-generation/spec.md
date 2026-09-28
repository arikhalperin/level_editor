# AI level generation

Complete target behaviour of the editor's AI level generation after this change is archived.

## Purpose

The editor can author a whole Hollow-Knight-style area by itself, using an AI model. The user
says what they want in words; the model writes the level; the editor refuses to hand over
anything it cannot prove is playable. What lands on the canvas is an ordinary level — the
same entities, the same file, the same editing, play and save behaviour as one placed by hand.

By default the model is OpenAI's. Because the editor speaks the OpenAI API rather than
anything proprietary, the same code drives a model server on the user's own machine: that is
a matter of changing the endpoint, not of a second implementation. The editor never installs,
downloads or bundles a local model.

## Reaching the model

The editor speaks the OpenAI chat-completions API: `POST {endpoint}/chat/completions` with
the configured model name. Both `https://` and plain `http://` endpoints work, so the same
client reaches OpenAI and a server on the user's own machine. Two settings govern it, both
remembered in the editor's config beside the other remembered paths:

| Setting | Default | Meaning |
| --- | --- | --- |
| Endpoint | `https://api.openai.com/v1` | Base URL. OpenAI, or any OpenAI-compatible server: Ollama, LM Studio, llama.cpp's server, vLLM, on this machine or another. |
| Model | `gpt-6-astra` | The model to ask for. Any model id the endpoint serves. Generation cannot start while it is empty. |

### The API key

The key is read from the `OPENAI_API_KEY` environment variable and sent as
`Authorization: Bearer`. It comes from the environment or it does not come at all:

- The editor never writes it to its config, never logs it, and never displays it. The dialog
  reports only whether a key was found, because a key in a config file is a plaintext secret
  that backup and sync tools copy, and an editor has no business holding one.
- With no key set and an endpoint that needs one, Generate fails before any request is sent,
  naming the variable so the user knows exactly what to set.
- A localhost endpoint needs no key, and none is sent to one.

### What leaves the machine

With the default endpoint the user's description and the generation prompts are sent to
OpenAI. That is inherent in asking OpenAI to write the level, and it is said plainly in the
README rather than left to be discovered. Pointing the endpoint at a local server means
nothing leaves the machine at all.

Where the server advertises JSON-schema-constrained output, requests use it, because a model
is far more likely to return usable JSON when the grammar is constrained. Where it does not,
the reply is parsed leniently: the first balanced JSON structure that actually parses is
taken, so a model that wraps its answer in prose or a fenced code block still works.

Every request runs on a worker thread. The UI thread keeps repainting throughout.

## Generating

`Level → Generate Level with AI…` opens the generation dialog. It is disabled while play
mode is running, with the same hover explanation `New Level` and `Load Level` use, because
it would otherwise replace the level being played.

The dialog carries the prompt — a free-text description of the area, its mood and its
difficulty — together with the endpoint and model settings, an optional seed, and the extent
to generate into, which defaults to the current level size when one is set. `Generate`
starts the run; `Cancel` closes the dialog and changes nothing.

The model authors the level JSON itself, including every coordinate. The editor expands no
plan of its own. It asks in stages:

1. **The area outline.** One request returns the chambers: each chamber's name, its
   rectangle, its intended difficulty and role, which chamber begins the area and which ends
   it, and — for every pair of adjoining chambers — the point at which they connect.
2. **Each chamber's entities.** One request per chamber returns that chamber's entity list in
   the level's own JSON shape, told the entry and exit points that chamber must put solid
   floor at.

Staging bounds each reply's length, keeps each one inside the shape the schema asks for, and
makes progress reportable. It takes no authorship away from the model: every polygon and
every placement in the delivered level is the model's, except where repair has said otherwise
and said so out loud.

The connection points matter more than they look. A chamber filled with no knowledge of its
neighbours will sit its floor at whatever height suits it, and two such floors meeting at a
boundary leave a step the character cannot climb — so the route dies at the seam and every
level needs repairing. Naming the connection point in the outline, and telling each chamber
which points it must provide floor at, is what makes a staged area actually connect.

The model may use only the editor's existing vocabulary — `wall_tool` and `blocker_tool`
polygons, plain polygons, and the `coin_tool`, `death_trap_tool`, `orc_tool` and `rope_tool`
bitmaps. It invents no new types, and a reply that names something else is rejected.

While a run is in flight the editor shows its stage — contacting the model, the outline,
chamber *n* of *m*, proving the route, repairing — with elapsed time and a `Cancel` that
ends the run and leaves the open level untouched.

## What a generated level is

A connected multi-chamber area: several chambers joined by corridors and shafts, a critical
path from the area's start to its end, and optional side branches carrying rewards. The
delivered level's `level_size` is the extent actually used, its `spawn` is the start of the
critical path and its `exit` is the end.

## Validation

Structural validation runs first, because it is cheap and catches most bad replies:

- every `bitmap_name` and `polygon_type` is one the toolbox defines;
- every polygon has at least three vertices, all finite and inside the level's extent;
- every bitmap size is positive and finite;
- every `rope_tool` entry has thickness 6 and length at least 60, as `LEVEL_FORMAT.md`
  requires.

A chamber that fails structural validation is re-asked. No entity that fails it ever reaches
the canvas.

## Proving the route

A level is delivered only once the editor has proven it playable — not estimated, proven.
The prover builds the same `World` play mode builds from the level's polygons and ropes,
then searches from `spawn` to `exit` by driving the real `Simulation` from `src/sim.rs`:
the same fixed 1/120 s step, the same constants, the same collision. Play mode and the
prover are the same simulation. If they ever diverge the proof is worthless, so the prover
adds no movement of its own and changes nothing about how the simulation behaves.

A resting state is one a next move could start from: standing on something, or clinging to a
wall. The search expands motion primitives from each reachable resting state — running off a
ledge, jumping at several hold lengths in both directions, dashing, wall jumping, climbing a
`wall_tool` face, and grabbing and releasing a rope — simulating each one and recording
where the character actually ends up. Reached states are discretised onto a grid so the
search terminates. When the exit is reached the input sequence that got there is kept as the
proof, and replaying it through a fresh simulation reaches the exit again.

When no route exists, the prover reports the ledge the route dies on: the reachable position
nearest the exit, and how far the nearest foothold beyond it lies.

## When the proof fails

The failure goes back to the model, phrased as something it can act on: which ledge the
route dies on, how far the nearest foothold is, and what the character can actually clear —
a 365 px jump, a 450 px dash, a 190 × 170 wall jump. The offending chamber is re-asked. This
happens at most twice.

If the route is still unproven, the editor repairs the geometry deterministically, making
the smallest edits that let the simulation through — lowering or extending a landing ledge,
narrowing a gap, adding a `wall_tool` face to climb — and re-proving after each edit, within
a bounded repair budget.

If repair cannot succeed within that budget, generation fails, says why, and leaves the open
level untouched. A level is never delivered unproven, and a half-repaired level is never
delivered at all.

The result panel says which it was: the model's own geometry, or the model's geometry
repaired, naming where. The user always knows whether they are looking at what the model
wrote.

## Failure

Every failure leaves the open level exactly as it was — its entities, size, background,
spawn and unsaved-changes state all untouched — and says what actually went wrong:

| Cause | What the user sees |
| --- | --- |
| No API key, where one is needed | That `OPENAI_API_KEY` is not set, before any request is sent. |
| Nothing listening | The endpoint that was tried and the connection error, within the connection timeout. |
| A key the endpoint rejects | The endpoint's own message, which is where an expired or wrong key arrives from. |
| Model name unknown to the server | The server's own message, and the model name that was asked for. |
| Unusable replies | That the model's replies could not be parsed or validated after its retries. |
| Unprovable after repair | That no playable route could be produced, and the ledge the route died on. |
| Cancelled | Nothing beyond the dialog closing. |

## Delivering the level

An accepted level replaces the current one. With unsaved changes this goes through the
editor's existing Save / Discard / Cancel confirmation — the same dialog `Exit` and
`New Level` use, carrying which action is pending — and `Cancel` leaves both the level and
the generated result untouched.

The replacement is a single undoable step: one Ctrl+Z restores the previous entities, level
size and spawn exactly. The view moves to the generated level's spawn.

## Level format

`LevelData` gains two optional fields, added exactly as `level_size` was:

```json
{
  "version": "1.0",
  "level_size": [6000.0, 3000.0],
  "spawn": [1200.0, 700.0],
  "exit": [5200.0, 2400.0],
  "entities": []
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `spawn` | `[f32; 2]?` | Where a run of this level begins. |
| `exit` | `[f32; 2]?` | Where the critical path ends. |

Both carry a serde default, so a level file written before this change loads unchanged, with
both `None`; an explicit `null` loads as `None` too. Hand-built levels simply have neither.

The Bevy game is unaffected: it ignores fields it does not know, and may adopt these later.

## Effect on play mode

When the open level carries a `spawn`, pressing Play starts the character there rather than
at the centre of the visible canvas. Everything else about play mode is unchanged: a left
click during play still moves the spawn point and respawns there, `R` still restarts the
run, and a level with no `spawn` — every hand-built level — still spawns at the centre of
the visible canvas exactly as before.

### Amendments

- `play-simulation` and the archived `play-spawn-at-view-centre` change state that the spawn
  point is never saved to the level JSON and always comes from the visible canvas. A level
  that carries a `spawn` now overrides that; the view-centre rule remains in force for every
  level that does not.
- `level-lifecycle` describes the unsaved-changes confirmation as carrying which action is
  pending. Generation adds one more pending action to it; the dialog's choices and their
  effects are unchanged.
- `New Level` still clears the play spawn, as `level-lifecycle` specifies.

## Testability

Everything about this is exercised without contacting a service. The model client sits behind
a trait, so the whole pipeline runs from recorded transcripts and stub replies; HTTP is
asserted against a local socket and TLS against a local TLS listener; and the validator,
prover and repairer are exercised directly on hand-built levels. Given the same replies a run
is deterministic and reproduces the same level; where an endpoint honours a seed, one is
passed.

One check cannot be faked and is not: a live generation against OpenAI with the user's own
key, confirming that a real model's output survives validation and the route proof. It sends
the user's prompt to OpenAI and costs them money, so it is run only with their go-ahead, and
it is recorded as actually run or as not run — never assumed from the offline tests passing.
