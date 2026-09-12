# Outcome

The editor's play mode gains the combat half of the game, so a level can be tested with the fights and pickups in it rather than only its geometry: the katana (forward / up / down slashes with recoil and pogo), orcs with their full charge-and-attack AI and power bar, club damage with knockback and i-frames, the shield, coins, and death pits. This is the follow-up change deferred by `level-size-and-play-simulation`, using the same source of truth — `/Users/arikha/bevy_prince_platformer`, with its constants transcribed verbatim.

# Scope

## Level entities become live

Bitmap entities, previously inert in play mode, now spawn behaviour keyed by their `bitmap_name`, exactly as the game's level importer does (`level1.rs`): `orc_tool` → an orc, `coin_tool` → a collectible coin, `death_trap_tool` → a death pit. Every other bitmap stays decorative and non-solid. Sizes come from the entity's own `size`, scaled by the game's factors (`COIN_LEVEL_SIZE_SCALE` 0.7, `DEATH_PIT_SIZE_SCALE` 4.0).

## Player combat

- **Katana** on `J` or `X`, with `ATTACK_INPUT_BUFFER_SEC` 0.12 press buffering and `SLASH_COOLDOWN` 0.28 between swings. Holding up slashes up; holding down while airborne slashes down; otherwise forward.
- Slash timing is derived from the game's clip table rather than guessed: forward is 15 frames at 60 fps (0.25 s) with the blade live over frames 5–8, i.e. 0.083–0.150 s; up and down are 8 frames at 32 fps (0.25 s) live over frames 2–5, i.e. 0.0625–0.1875 s.
- Hitboxes are the game's: forward `SLASH_FORWARD_HALF_W/H` 62×50 centred `SLASH_FORWARD_REACH` 54 ahead and `SLASH_FORWARD_Y` 8 up; up and down `SLASH_VERTICAL_HALF_W/H` 50×44 centred `SLASH_VERTICAL_REACH` 62 above or below. The box is fixed at the facing the swing started with.
- A landed forward slash recoils the player at `SLASH_RECOIL_SPEED` 400 away from the target for `SLASH_RECOIL_SECS` 0.1. A landed **airborne** down slash pogos to `POGO_HEIGHT` 160 and restores the air dash. Ground speed is multiplied by `SLASH_GROUND_SPEED_MULT` 0.35 during a swing. Each orc can be hit at most once per swing.
- Slashing is refused while dashing, climbing, or already swinging.

## Orcs

- State machine as in `behavior.rs`: **Idle** → **PreparingCharge** (1.0 s wind-up, skipped to 0 when coming out of a sword freeze) → **Charging** (launch 200 px/s, accelerate 1000 px/s² to a 250 px/s cap) → **Attacking**. The orc attacks when the player is within `ORC_ATTACK_RANGE_PX` 105, begins a charge within 400, and leaves Attacking beyond `ORC_ATTACK_RANGE_LEAVE_PX` 160.
- A charge stops `ORC_WALL_STOP_DISTANCE_PX` 50 before a wall, at a ledge (`ORC_LEDGE_PROBE_FORWARD_PX` 52 ahead, ground probe `ORC_GROUND_PROBE_FEET_OFFSET_Y` 68 down for at most `ORC_GROUND_PROBE_MAX_DIST` 96), and returns to Idle below `ORC_MIN_MOVING_SPEED` 15. Orcs fall under the same gravity as the player and collide with the same polygons.
- **Power**: `ORC_MAX_POWER` 100, each sword hit removing `ORC_SWORD_POWER_FRACTION` 0.25 of max — four hits to defeat. A non-lethal hit freezes the orc for `ORC_HIT_FREEZE_SEC` 0.35 and knocks it back `ORC_SWORD_KNOCKBACK_X/Y` 260/120 held for `ORC_SWORD_KNOCKBACK_SECS` 0.3. Defeat scores `ORC_DEFEAT_SCORE` 100.
- **Club**: the attack cycle is 10 frames at `ORC_ATTACKING_ANIM_FPS` 12 (0.833 s); the club deals damage only over frames `ORC_CLUB_ACTIVE_FRAME_MIN`–`MAX` 3–5, i.e. 0.25–0.5 s into each swing. A hit needs the player within `ORC_CLUB_HIT_MAX_SEPARATION_X/Y_PX` 112×82, costs `ORC_CLUB_DAMAGE` 1, and knocks the player back `ORC_CLUB_KNOCKBACK_X/Y` 300/170. `ORC_REATTACK_DELAY_SEC` 0.45 separates consecutive swings.

## Player condition

- Health starts at `INITIAL_HEALTH` 3. A club hit locks movement and attacks for `HURT_LOCKOUT_SECS` 0.45 and grants `PLAYER_CLUB_IFRAMES` 1.3 s of invulnerability.
- **Shield** on `L`: invulnerable for `SHIELD_ACTIVE_SECS` 2.0, then locked out for `SHIELD_COOLDOWN_SECS` 5.0. While it is up, club damage is refused.
- Death — health reaching 0 or touching a death pit — respawns the character at the spawn point with full health and **resets every orc and coin**, so each run tests the level from a clean state.

## Coins

Touching a coin collects it, scoring `COIN_PICKUP_POINTS` 40. Its pickup box is the entity's `size` scaled by `COIN_LEVEL_SIZE_SCALE` 0.7.

## Editor surface

- The status overlay gains health, score, coins collected out of the level's total, and the shield's state (ready / active with time left / cooling down).
- **F2** toggles a combat debug view drawing the live slash hitbox, each orc's attack range and its club-active danger window.
- Play mode stays strictly non-destructive: combat never alters the level's entities. Defeating an orc or collecting a coin changes only per-session play state, and `has_unsaved_changes()` is untouched.
- The help window and the archived `editor-help-screen` spec gain the new keys (`J`/`X`, `L`, `F2`); the archived `play-simulation` spec is amended where combat supersedes it (bitmap entities are no longer inert; the input list and overlay grow).

# Non-goals

- Sprites, sprite animation, VFX (sparks, afterimages, dust), hit-stop, screen shake and audio. Orcs, coins and pits are drawn as plain shapes, as the character already is.
- The ninja-star tool and tool switching (`ToolKind::NinjaStar`, keys 1/2/Q/E). The katana is the whole of combat here.
- Lives and game-over (`INITIAL_LIVES`): death respawns unconditionally, so attempts are unlimited.
- Checkpoints — the editor's toolbox has no checkpoint tool, so no level entity maps to one.
- Score persistence between play sessions, and any HUD beyond the existing overlay line.
- Editing `/Users/arikha/bevy_prince_platformer`, including its `LEVEL_JSON_VIEWPORT_HEIGHT = 720.0` Y-flip, still outstanding from the previous change.
- Bit-identical parity with the game's Bevy/Rapier/Tnua systems; this remains an independent kinematic reimplementation sharing the constants.
- Orc-vs-orc collision, orc knockback into other orcs, and the orc power-bar sprite (the bar's value is shown in the debug view instead).

# Acceptance examples

- A1 — Entities spawn: entering play in a level containing `orc_tool`, `coin_tool` and `death_trap_tool` bitmaps creates an orc, a coin and a death pit at those positions; other bitmap names create nothing.
- A2 — Attack input: `J` or `X` swings the katana; a press within `ATTACK_INPUT_BUFFER_SEC` 0.12 of becoming able to attack is honoured; a second swing inside `SLASH_COOLDOWN` 0.28 is refused.
- A3 — Slash direction: holding up slashes up; holding down while airborne slashes down; otherwise forward. Left-click still moves the spawn point and never attacks.
- A4 — Slash timing: the forward blade is live 0.083–0.150 s into a 0.25 s swing, up/down 0.0625–0.1875 s; outside that window nothing is hit.
- A5 — Slash reach: an orc centred 54 px ahead within the 62×50 half-extents is hit; one beyond the box is not. The hitbox keeps the facing the swing began with.
- A6 — Recoil and pogo: a landed forward slash pushes the player away at 400 px/s for 0.1 s; a landed airborne down slash sets upward velocity for a 160 px rise and restores the air dash. One orc takes at most one hit per swing.
- A7 — Orc AI: an orc idles until the player is within 400 px, winds up 1.0 s, charges to a 250 px/s cap, and switches to Attacking within 105 px, returning to Idle beyond 160 px.
- A8 — Orc stops safely: a charging orc halts 50 px before a wall and at a ledge rather than walking off, and falls under gravity like the player.
- A9 — Orc power: four sword hits defeat an orc; each non-lethal hit freezes it 0.35 s and knocks it back 260/120 for 0.3 s; defeat adds 100 to the score.
- A10 — Club window: the orc's club damages only between 0.25 s and 0.5 s of its 0.833 s swing, and only when the player is within 112×82 px.
- A11 — Taking damage: a club hit costs 1 health, knocks the player back 300/170, locks control for 0.45 s and grants 1.3 s of invulnerability during which further club hits do nothing.
- A12 — Shield: `L` makes the player invulnerable for 2.0 s; club hits during it are refused; it cannot be raised again for 5.0 s after it ends.
- A13 — Coins: touching a coin removes it and adds 40 to the score; the pickup box is the entity size × 0.7.
- A14 — Death pits: touching one kills the player regardless of health.
- A15 — Death and reset: reaching 0 health or touching a pit respawns the character at the spawn point with 3 health, and restores every defeated orc and collected coin.
- A16 — Overlay: while playing, health, score, coins collected out of the level total, and the shield state are shown.
- A17 — Debug view: `F2` toggles drawing of the live slash hitbox, each orc's 105 px attack range and its club-active window; off by default.
- A18 — Non-destructive: a play session containing kills and pickups leaves the entity list byte-identical and `has_unsaved_changes()` unchanged; stopping and restarting play restores every orc and coin.
- A19 — Frame-rate independence: the same inputs at 60 Hz and 144 Hz produce the same combat outcome (same hits landed, same health, same score) within one fixed step.
- A20 — Help stays accurate: the help window lists `J`/`X`, `L` and `F2` with their effects, keeping the archived guarantee that it documents every key the editor handles.

# Constraints and invariants

- Every tuning value is a named constant transcribed from `/Users/arikha/bevy_prince_platformer/src/game/config.rs` with the game's own name, extending `src/game_config.rs`. Three timings are **derived** from the game's animation tables because they live in clip data rather than config, and each records its derivation: the slash durations and hit windows (from the clip table in `sugisan/animation.rs`) and the orc's club-active window (frames 3–5 of a 10-frame strip at 12 fps).
- Combat lives in the pure simulation layer with no egui `Context`/`Ui`, advanced by the existing 1/120 s fixed step, so every timing above is unit-testable headlessly.
- Play mode never mutates `entities`, never calls `save_state()`, and never moves `last_save_hash`. Orc and coin state is per-session and rebuilt from the level on every entry to play.
- The level format is unchanged: no new fields, and `LevelData` is untouched.
- Orcs collide with the same polygon colliders as the player and use the same gravity and terminal velocity.
- No new dependencies; `eframe`/`egui` 0.27 only.
- `cargo build` and `cargo test` keep passing; the existing 72 tests stay green.

# Decisions

- D1 — Workspace: current directory. Only an unrelated `README.md` edit is uncommitted.
- D2 — Attack is bound to `J` / `X` only; left-click keeps moving the spawn point, so the archived play-simulation guarantee is preserved (user choice).
- D3 — Death respawns at the spawn point and resets all orcs and coins, giving a clean run each attempt (user choice).
- D4 — A toggleable combat debug view draws slash hitbox, orc attack range and club-active window (user choice); bound to `F2`, chosen because it sits beside the existing `F1`/`F5` and collides with no movement key.
- D5 — (Agent) The orc's club-active window is expressed in seconds (0.25–0.5 s of a 0.833 s swing) rather than animation frames, since this simulation has no sprites; the numbers are derived from the game's frame indices and FPS, so they track the same behaviour.
- D6 — (Agent) Slash durations and hit windows likewise come from the game's clip table, not from invented values.
- D7 — (Agent) Orcs are simulated with the same kinematic body and collision routine as the player, rather than a second physics path, so they stand on and are blocked by exactly the level geometry the player is.
- D8 — (Agent) The ninja star, lives and checkpoints are excluded as listed in Non-goals; each is a separable system and none is needed to test a level's combat layout.
- D9 — (Agent) Amending the archived `play-simulation` and `editor-help-screen` specs is in scope, because this change makes statements in both untrue (inert bitmaps, the key list). Leaving them stale would be a silent regression of an archived guarantee.

# Open questions

None. Shared understanding (scope, D1-D9, A1-A20, non-goals, current-directory workspace) was explicitly confirmed by the user on 2026-09-12.

# Verification expectations

- `cargo build` succeeds with no new warnings in the new code; `cargo test` passes.
- Unit tests on the pure combat layer, all headless and asserting the transcribed or derived constants: slash cooldown and input buffer (A2); direction selection (A3); the exact hit windows and that no hit lands outside them (A4); hitbox extents hitting and missing at the boundary, and facing fixed at swing start (A5); recoil velocity and duration, pogo height, air-dash restore, one-hit-per-orc-per-swing (A6); the orc state machine's four transitions with their distance thresholds and 1.0 s wind-up, and the 250 px/s cap (A7); wall stop at 50 px and ledge stop (A8); four hits to defeat, freeze 0.35 s, knockback 260/120 for 0.3 s, score 100 (A9); club damage only inside 0.25–0.5 s and only within 112×82 (A10); damage cost, knockback, 0.45 s lockout and 1.3 s i-frames refusing a second hit (A11); shield active 2.0 s refusing damage and locked out 5.0 s (A12); coin pickup box and score (A13); pit death (A14); death resetting health, orcs and coins (A15); identical outcomes at 60 Hz and 144 Hz (A19).
- Unit tests on the level→entity mapping: the three `bitmap_name`s spawn the right things and any other name spawns nothing (A1).
- Verifier code inspection in `main.rs`: overlay contents (A16); the `F2` toggle and the debug drawing (A17); every entity-mutating path still gated during play and `has_unsaved_changes` untouched by a session with kills and pickups (A18); help content and the amended specs (A20).
- Smoke run: the app launches, stays alive and does not panic.
