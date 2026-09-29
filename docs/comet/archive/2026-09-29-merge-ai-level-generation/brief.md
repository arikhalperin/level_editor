# Outcome

The AI level generator becomes part of the editor. Today it is finished, verified 24/24
and archived, but it exists only on `comet/ai-level-generation` in a separate worktree:
nothing on `main` can reach it, and anyone who clones the repository gets an editor
without it. After this change `main` has both halves of the recent work — a generator that
writes a level and proves it playable, and blockers that can be dressed in a seamlessly
tiled pattern — and they work together rather than in separate histories.

The branch forked at `a072648`, before the three pattern commits, so the merge is not a
formality. Three files collide.

# Scope

- Merge `comet/ai-level-generation` into `main` as a real merge, preserving both its
  commits: the feature and the archive that records how it was verified.
- Resolve the three conflicts. All are two sides adding in the same place rather than
  disagreeing, with exactly one real integration:
  - `src/main.rs`, `initial_load`: the branch gathered the startup loads into one method
    calling background, level and remembered model settings. The pattern work added a
    fourth, the gallery. It must call all four.
  - `src/main.rs`, the `Level` menu: the branch adds `Generate Level with AI…` at the top,
    the pattern work adds three items below `Level Size…`. Both stay, in that order.
  - `src/help.rs`: both sides added entries. Both sets stay, and the branch's revised
    `Play / Stop` wording replaces the pattern work's, because it is now the accurate
    one — a generated level carries its own start, so play no longer always begins at the
    centre of the view. The exhaustive menu list in its test gains every new item.
  - `src/level_data.rs`: both sides appended tests, the branch's for `spawn` and `exit`,
    the pattern work's for `pattern`. Both sets stay.
- Bring the branch's live capability spec at `docs/comet/specs/ai-level-generation/` and
  its archived artifacts onto `main`, so the record arrives with the code.

# Non-goals

- No behaviour is changed on either side beyond what resolving a conflict requires. This
  change merges; it does not improve.
- The generator's documented known limitations are not addressed: that its repair prose
  promises more than the code does, that only one prompt and one model family have been
  tried live, that nobody has played a generated level, and that a self-naming sprite with
  a malformed vertex list is accepted with its vertices discarded. They are recorded in the
  archived verification and stay as they are.
- The branch and its worktree are kept, not removed. Confirmed by the user.
- The thirty uncommitted tooling files — the Comet skill update under `.github`,
  `.gitignore`, `.comet/config.yaml` and `.codegraph` — are not touched and not committed.
  The branch changes none of them, which is why the merge is not blocked by them.
- No attempt to reconcile the two features beyond making them coexist. A generated level
  does not gain patterns and a patterned blocker does not gain a generated spawn.

# Acceptance examples

- Scenario: After the merge, `main` contains the generator's six modules — `ai_client`,
  `level_gen`, `traversal`, `repair`, `generate_ui` and `undo` — and `Level → Generate
  Level with AI…` appears in the menu, disabled while play mode runs.
- Scenario: The pattern work survives intact: a polygon still carries an optional pattern
  path, the gallery still remembers twelve, and `quilt` still synthesises a tile that wraps
  against itself.
- Scenario: `initial_load` performs all four startup loads — the last background, the last
  level, the pattern gallery and the remembered model settings — so neither side's
  remembered state is lost at launch.
- Scenario: `start_play` prefers a level's own start, then the centre of the visible
  canvas, then the last spawn, then `(100, 100)`, so a generated level begins where it was
  proved playable from and any other level behaves as it did before.
- Scenario: The help screen lists every menu item the editor offers, from both sides, and
  its exhaustive assertion passes rather than being relaxed.
- Scenario: A level file round-trips a polygon's `pattern` and the level's `spawn` and
  `exit` together, and a file written before either existed still loads with all three
  absent.
- Scenario: `cargo test` passes with both sides' tests present — at least 273 from `main`
  and at least 343 from the branch, the union rather than either alone — and the one
  `#[ignore]`, the live paid model request, remains ignored.
- Scenario: No test from either side is removed, renamed or weakened to make the merge fit.
- Scenario: `cargo build` succeeds with no warning beyond the union of the two baselines.
- Scenario: The thirty uncommitted tooling files are in the same state after the merge as
  before it, and none is included in the merge commit.

# Constraints and invariants

- A real merge, not a squash: the branch's archive commit is part of how this work is
  recorded and must survive in history.
- Conflict resolution keeps both sides wherever both added something. The only place one
  side's text replaces the other's is the `Play / Stop` help wording, and only because the
  branch's is now the true description.
- The merge must not be completed with a failing build or suite. If resolving a conflict
  reveals a real incompatibility rather than an additive collision, that is a finding to
  report, not something to paper over by deleting one side.
- `main` must stay the only place the merge lands; the branch and worktree are read from,
  never written to.
- Nothing in `.github`, `.gitignore`, `.comet` or `.codegraph` may be staged.

# Decisions

- **Isolation: current directory**, on `main`, confirmed by the user. It is where the merge
  has to land, and the branch touches none of the thirty dirty files, so they neither
  block it nor risk being swept in.
- **The branch and worktree are kept afterwards**, confirmed by the user, so the exact
  pre-merge state stays available if the merged result surprises anyone.
- **A merge commit rather than a rebase.** Rebasing would replay the branch's two commits
  onto `main` and rewrite the archive commit's identity; the verified history is worth more
  than a linear graph here.
- **The branch's `Play / Stop` help wording wins.** Not a preference: the pattern work's
  wording said play always begins at the centre of the visible canvas, and that stopped
  being true when a level gained the ability to carry its own start.
- **The generator already builds on the pattern work's spawn logic.** Its `start_play`
  is `level_spawn.or_else(|| view_centre()).or(play_spawn).unwrap_or(…)` — the chain from
  `a072648` with one entry prepended — so the two are designed to coexist and the merge is
  not reconciling a disagreement.

# Open questions

None. Both decisions affecting the outcome were confirmed before Build.

# Verification expectations

- `cargo build` succeeds with no new warnings beyond the union of the two baselines.
- `cargo test` passes, with the union of both sides' tests present and the single live
  model test still ignored.
- A test-name diff against both `main` and `comet/ai-level-generation` shows only
  additions, proving nothing was dropped to make the merge fit.
- `git log` shows the branch's two commits reachable from `main`.
- `git status` shows the thirty tooling files unchanged and unstaged.
