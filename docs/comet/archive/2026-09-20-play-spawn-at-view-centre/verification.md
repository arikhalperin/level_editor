---
generated_from_state_version: 8
---

# Verification

## Current result

- Result: **Archived**
- Verification status: **Checks completed; result confirmed**
- Goal cycle: 1
- Iteration: 1
- Verifier attempt: 1
- Completed: 2026-09-20T09:36:58.175Z
- Summary: Independent read-only Verifier assessed all 8 acceptance items against the actual implementation and passed 8/8. It re-ran cargo build (exit 0, 10 warnings all pre-existing, none touching the new code) and cargo test (201 passed, 0 failed) itself, and confirmed via git that 9 tests were added and none removed, with the change confined to src/main.rs (+217/-18) and two strings in src/help.rs. It independently derived the world/screen convention from the click path and renderer rather than trusting the Builder, confirmed the canvas rect genuinely excludes the menu bar because both available_rect() calls sit after the TopBottomPanel and before the CentralPanel, and scrutinised the frame-ordering of canvas_rect, judging the one-frame staleness correct because scroll_offset is stale in the same way. Five non-blocking risks recorded, including an unclamped spawn outside level bounds that predates this change.

## Acceptance

| ID | Result | Source | Criterion | Reason |
| --- | --- | --- | --- | --- |
| A1 | passed | brief.md | Scenario: With the view scrolled so the visible canvas covers world x 800-1600 and y 400-1000, pressing F5 starts the character at world (1200, 700), the centre of that visible area. | start_play (main.rs:787-795) takes view_centre(). Test play_starts_the_character_at_the_centre_of_the_visible_canvas (main.rs:2432-2445) uses canvas (0,40)-(800,640) with offset (800,360), giving visible world x 800..1600 and y 400..1000, and asserts spawn (1200,700) and play_spawn == Some((1200,700)). Offsets are integral so pixel_offset()'s round() (scroll.rs:110-112) introduces no drift. |
| A2 | passed | brief.md | Scenario: With the view at the level origin so the visible canvas covers world x 0-800 and y 0-600, pressing F5 starts the character at world (400, 300). | play_starts_at_the_centre_with_the_view_at_the_level_origin (main.rs:2446-2460) asserts (400,300) for canvas (0,0)-(800,600) at zero offset. Verifier noted that fixture's y=0 origin cannot occur in the real app, but the realistic y-axis case is carried by A1's fixture and by the_centre_is_the_canvas_centre_not_the_window_centre (main.rs:2461-2474), which asserts (640,400) against the window-centre answer (640,380). |
| A3 | passed | brief.md | Scenario: The user clicks the canvas at world (50, 50), scrolls far away so that point is off screen, then presses F5. The character starts at the centre of the new visible area, not at (50, 50). | a_click_made_before_play_no_longer_decides_where_the_run_begins (main.rs:2475-2487) sets last_click_pos=(50,50) and still asserts (1200,700) plus an explicit assert_ne! against (50,50). start_play no longer reads last_click_pos at all; the diff removes it from the spawn chain. |
| A4 | passed | brief.md | Scenario: After play has started at the view centre, pressing R restarts the run with the character back at that same view-centre point. | restarting_the_run_returns_the_character_to_the_view_centre (main.rs:2488-2503) moves the player to (4000,2000), calls restart(), and asserts return to (1200,700). Chain independently traced: PlaySession::restart -> Simulation::respawn -> respawn_at(self.spawn) (combat.rs:770-774, sim.rs:442-452), spawn set from PlaySession::new at main.rs:804. The R handler (main.rs:1397-1401) calls exactly play.restart(). |
| A5 | passed | brief.md | Scenario: While playing, a left click on the canvas at a visible point still moves the spawn to that point and respawns the character there. | Extraction verified behaviour-preserving: the removed inline block and move_play_spawn (main.rs:809-822) are line-for-line identical, and the call site (main.rs:1845) passes the same two values under the unchanged guard (main.rs:1843-1845). Effect is covered by a_click_while_playing_still_moves_the_spawn_and_respawns_there (main.rs:2504-2519). Caveat recorded: no test drives a real click through the dispatch path; that half rests on the byte-identical diff. |
| A6 | passed | brief.md | Scenario: Starting play from the Play menu places the character at the same point that F5 would. | Play menu (main.rs:1305-1312) and F5 (main.rs:1390-1392) both call the same toggle_play (main.rs:837-843). Verifier independently confirmed both sites precede the scroll_offset update (main.rs:1426-1428) and the canvas_rect assignment (main.rs:1549), so on any frame both read identical inputs and must yield the identical spawn. The test proves toggle_play determinism; the equality claim rests on that code read. |
| A7 | passed | brief.md | Scenario: Stopping play restores the editor's scroll position to exactly where it was when play began. | stop_play is unchanged by this diff and restores via camera::restore then recomputes scroll_offset (main.rs:824-835); capture/restore are exact copies with velocity zeroed (camera.rs:110-119). stopping_play_restores_the_view_it_began_with (main.rs:2536-2549) wanders the camera to (9000,1200) and asserts the exact pre-play offset returns. |
| A8 | passed | brief.md | Scenario: When no canvas rectangle has been recorded yet, starting play falls back to the remembered `play_spawn`, and to `(100, 100)` when there is none, so the spawn is always defined. | view_centre() returns None when canvas_rect is None (main.rs:783-785) and start_play chains .or(play_spawn).unwrap_or((100,100)) (main.rs:792-795). without_a_laid_out_canvas_play_falls_back_to_the_last_spawn (main.rs:2550-2569) covers both branches. canvas_rect defaults to None (main.rs:186) and new_level correctly does not reset it. |

## Checks

| Check | Command | Working directory | Status | Exit | Duration |
| --- | --- | --- | --- | ---: | ---: |
| cargo build | build | . | passed | 0 | 514 ms |
| cargo test | test | . | passed | 0 | 3291 ms |

### Builder-reported evidence

These are Builder reports, not Runtime check receipts or independent verification results.

- cargo build: passed — Compiles. 12 warning lines, identical to the pre-change baseline; none reference canvas_rect, view_centre, start_play or move_play_spawn.
- cargo test: passed — 201 unit tests plus 1 integration test, 0 failed. Includes the 9 new tests.
- mutation check: passed — Reverting the spawn chain to last_click_pos fails 6 of 9 new tests, confirming they discriminate rather than passing vacuously.
- manual GUI run: not-run — The editor is an eframe GUI app; play placement was verified through headless unit tests rather than by driving the window.
- Known limitation: The per-frame recording of canvas_rect lives inside eframe::App::update, which the existing test suite never invokes (no test in this repo constructs an eframe::Frame). That one assignment is therefore covered by reasoning and by the GUI itself, not by an automated test. The value it records is the same ctx.available_rect() already passed to minimap::layout and the play camera on the same line, so a wrong rect there would also break existing minimap and camera behaviour.
- Known limitation: A1 and A2 assert exact world coordinates derived from a synthetic canvas rectangle; they verify the arithmetic, not the real window's layout.
- Known limitation: Acceptance A6 is covered by testing toggle_play, which is the single function both the F5 key handler and the Play menu item call. The two call sites themselves are one-line calls inside update() and are not separately exercised.

## Blockers

_None._

## Risks and skipped work

- canvas_rect (assigned main.rs:1549) is one frame stale relative to the play triggers (main.rs:1310, 1390). It is self-consistent because scroll_offset is equally stale, so the pair describes the view the user was looking at when they pressed the key. Residual exposure: a window resize in the same frame as the trigger gives a spawn off by half the resize delta, and F5 on the literal first frame falls back via A8. A more robust form would derive the rect from ctx.screen_rect() minus the already-stored menu_bar_rect (main.rs:1323) before the triggers.
- No test drives the real input path. F5, the Play menu item and the click-during-play are all tested via the methods they call, not through a headless frame. The project already has a headless-egui frame helper (minimap_frame, main.rs:2206-2218), so a stronger test was available. This most affects A5 and A6.
- A2's fixture canvas starts at y=0, a layout the app cannot produce, making it weaker evidence than it appears; the y-axis claim is carried by A1 and the menu-bar test.
- The spawn is not clamped to the level bounds (main.rs:787-795). Scrolled past the level edge, Play now drops the character outside the level with nothing beneath them. The previous click-based behaviour had the same exposure and the brief sets no requirement here, so this is a UX question for the user rather than an acceptance failure.
- The mutation check could not be executed by the read-only Verifier; it instead proved discrimination from the fixture's initial state (EditorState::default() has last_click_pos: None and play_spawn: None, main.rs:189/206), so a reverted implementation would yield (100,100) or (50,50) and fail. The Builder separately observed the red run: 6 of 9 new tests failed on the reverted chain.

## Previous iterations

| Goal cycle | Iteration | Attempt | Outcome | Unresolved | Summary | Completed |
| ---: | ---: | ---: | --- | --- | --- | --- |
| 1 | 1 | 1 | pass | — | Independent read-only Verifier assessed all 8 acceptance items against the actual implementation and passed 8/8. It re-ran cargo build (exit 0, 10 warnings all pre-existing, none touching the new code) and cargo test (201 passed, 0 failed) itself, and confirmed via git that 9 tests were added and none removed, with the change confined to src/main.rs (+217/-18) and two strings in src/help.rs. It independently derived the world/screen convention from the click path and renderer rather than trusting the Builder, confirmed the canvas rect genuinely excludes the menu bar because both available_rect() calls sit after the TopBottomPanel and before the CentralPanel, and scrutinised the frame-ordering of canvas_rect, judging the one-frame staleness correct because scroll_offset is stale in the same way. Five non-blocking risks recorded, including an unclamped spawn outside level bounds that predates this change. | 2026-09-20T09:36:58.175Z |



## Conclusion

Independent read-only Verifier assessed all 8 acceptance items against the actual implementation and passed 8/8. It re-ran cargo build (exit 0, 10 warnings all pre-existing, none touching the new code) and cargo test (201 passed, 0 failed) itself, and confirmed via git that 9 tests were added and none removed, with the change confined to src/main.rs (+217/-18) and two strings in src/help.rs. It independently derived the world/screen convention from the click path and renderer rather than trusting the Builder, confirmed the canvas rect genuinely excludes the menu bar because both available_rect() calls sit after the TopBottomPanel and before the CentralPanel, and scrutinised the frame-ordering of canvas_rect, judging the one-frame staleness correct because scroll_offset is stale in the same way. Five non-blocking risks recorded, including an unclamped spawn outside level bounds that predates this change.
