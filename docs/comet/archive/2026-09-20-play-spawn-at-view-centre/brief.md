# Outcome

Pressing Play starts the character at the centre of whatever part of the level is on
screen at that moment. Testing a spot becomes: scroll until you can see it, press Play.
No click is needed first, and a stale click somewhere else in the level can no longer
decide where the run begins.

Today `start_play` takes the spawn from `last_click_pos`, falling back to the remembered
`play_spawn` and then to a hardcoded `(100, 100)`. That means the character can appear
entirely off screen — the common case when the view has been scrolled since the last
canvas click.

# Scope

- `start_play` in `src/main.rs` derives the spawn from the visible canvas rather than
  from `last_click_pos`.
- The editor records the canvas rectangle each frame so `start_play` can read it, in the
  same spirit as the already-stored `minimap_layout`.
- `play_spawn` is still set to the resulting point, so an in-run restart and a death
  respawn both return the character there.
- The help screen's description of what Play does is updated to say where the character
  appears.
- Unit tests covering the new placement, at more than one scroll offset.

# Non-goals

- Left click during play still moves the spawn point and respawns there; that behaviour
  is unchanged, and so is its help entry.
- No change to camera follow, camera containment, or the view restore performed by
  `stop_play`.
- No change to the level format. The spawn point is still not saved to the level JSON.
- No new UI, no on-canvas spawn marker, no setting to choose the old behaviour.
- `last_click_pos` keeps every other job it has (double-click detection, tool placement).

# Acceptance examples

- Scenario: With the view scrolled so the visible canvas covers world x 800-1600 and
  y 400-1000, pressing F5 starts the character at world (1200, 700), the centre of that
  visible area.
- Scenario: With the view at the level origin so the visible canvas covers world
  x 0-800 and y 0-600, pressing F5 starts the character at world (400, 300).
- Scenario: The user clicks the canvas at world (50, 50), scrolls far away so that point
  is off screen, then presses F5. The character starts at the centre of the new visible
  area, not at (50, 50).
- Scenario: After play has started at the view centre, pressing R restarts the run with
  the character back at that same view-centre point.
- Scenario: While playing, a left click on the canvas at a visible point still moves the
  spawn to that point and respawns the character there.
- Scenario: Starting play from the Play menu places the character at the same point that
  F5 would.
- Scenario: Stopping play restores the editor's scroll position to exactly where it was
  when play began.
- Scenario: When no canvas rectangle has been recorded yet, starting play falls back to
  the remembered `play_spawn`, and to `(100, 100)` when there is none, so the spawn is
  always defined.

# Constraints and invariants

- The editor's world/screen mapping is `world = screen + scroll_offset`; the spawn is
  therefore `canvas_rect.center() + scroll_offset`.
- The canvas rectangle must be the one the canvas actually occupies (`ctx.available_rect()`
  as taken at the canvas panel), not the whole window, so the menu bar does not bias the
  centre downward.
- Placement must be unit-testable headlessly, following the existing pattern where the
  camera module is pure and takes the panel rectangle as an argument.
- Starting play must not modify the level, push an undo entry, or mark the level dirty.
- `stop_play` must still restore the pre-play view via `view_before_play`.

# Decisions

- **Spawn at the centre of the visible canvas.** Confirmed by the user over top-centre
  and upper-third alternatives. It is the literal reading of "current seen position" and
  is symmetric whichever way the view was scrolled.
- **Left click during play keeps moving the spawn.** Confirmed by the user. It does not
  conflict with the new start behaviour: the view centre decides where a run begins, a
  click decides where it resumes.
- **`play_spawn` is still written at play start**, so restart and death respawn stay
  consistent with the point the run began from.
- **Isolation: current directory**, on `main`, matching the nine previous Native changes
  in this project.

# Open questions

None. Every decision affecting user-visible behaviour was confirmed before Build.

# Verification expectations

- `cargo test` passes, including new tests that assert the spawn equals the visible
  canvas centre in world coordinates at two different scroll offsets.
- A test asserts that a click before play no longer determines the spawn.
- A test asserts the click-during-play respawn still works.
- `cargo build` succeeds with no new warnings.
- Existing play, camera, minimap and new-level tests continue to pass unchanged, except
  where they encode the old spawn rule and are updated deliberately.
