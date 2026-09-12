# Play combat

Complete target behaviour of combat inside the editor's play mode after this change is archived. It extends the `play-simulation` capability, which owns movement, the spawn point, the fixed timestep and the non-destructive guarantee.

## Source and fidelity

Behaviour reproduces `/Users/arikha/bevy_prince_platformer`. Its constants are transcribed into `src/game_config.rs` under the game's own names. Three timings live in the game's animation tables rather than its config and are therefore **derived**, each recording the derivation at its definition:

| Derived value | From |
| --- | --- |
| Forward slash: 0.25 s long, blade live 0.083–0.150 s | 15 frames at 60 fps, hit frames 5–8 (`sugisan/animation.rs` clip table) |
| Up / down slash: 0.25 s long, blade live 0.0625–0.1875 s | 8 frames at 32 fps, hit frames 2–5 |
| Orc club active 0.25–0.5 s of a 0.833 s swing | frames 3–5 of a 10-frame strip at `ORC_ATTACKING_ANIM_FPS` 12 |

As with movement, this is an independent kinematic implementation, not a port: the game's combat runs on Bevy ECS with Rapier shape queries, which cannot run inside egui. The constants are exact; outcomes are close, not bit-identical.

## Level entities

On entering play, bitmap entities are read and mapped by `bitmap_name`, mirroring the game's importer:

| `bitmap_name` | Becomes | Size |
| --- | --- | --- |
| `orc_tool` | An orc | the entity's `size` |
| `coin_tool` | A collectible coin | `size` × `COIN_LEVEL_SIZE_SCALE` 0.7 |
| `death_trap_tool` | A death pit | `size` × `DEATH_PIT_SIZE_SCALE` 4.0 |

Any other bitmap name creates nothing and stays decorative and non-solid. All of this state is per-session: it is rebuilt from the level every time play begins, and nothing about it is written back to the level or to the level file.

## Katana

`J` or `X` swings. A press is buffered for `ATTACK_INPUT_BUFFER_SEC` 0.12 and fires as soon as swinging is allowed; swings are at least `SLASH_COOLDOWN` 0.28 apart. Swinging is refused while dashing, climbing, or mid-swing.

Direction: up while up is held; down while down is held **and** airborne; otherwise forward. The hitbox is fixed at the facing the swing started with and does not turn mid-swing.

| Slash | Half-extents | Centre |
| --- | --- | --- |
| Forward | `SLASH_FORWARD_HALF_W/H` 62 × 50 | `SLASH_FORWARD_REACH` 54 ahead, `SLASH_FORWARD_Y` 8 above centre |
| Up | `SLASH_VERTICAL_HALF_W/H` 50 × 44 | `SLASH_VERTICAL_REACH` 62 above |
| Down | `SLASH_VERTICAL_HALF_W/H` 50 × 44 | `SLASH_VERTICAL_REACH` 62 below |

While a swing is live on the ground, horizontal speed is multiplied by `SLASH_GROUND_SPEED_MULT` 0.35. A landed forward slash recoils the player at `SLASH_RECOIL_SPEED` 400 away from the target, held `SLASH_RECOIL_SECS` 0.1. A landed **airborne** down slash pogos the player to `POGO_HEIGHT` 160 and restores the air dash. Each orc is hit at most once per swing.

## Orcs

An orc is a kinematic body using the same collision routine, gravity and terminal velocity as the player, so it stands on and is blocked by exactly the level's polygons.

**States** — Idle, PreparingCharge, Charging, Attacking:

- Idle → Attacking when the player is within `ORC_ATTACK_RANGE_PX` 105.
- Idle → PreparingCharge when the player is within 400.
- PreparingCharge holds for a 1.0 s wind-up, shortened to 0 when the orc is coming out of a sword freeze, then → Charging.
- Charging launches at 200 px/s and accelerates at 1000 px/s² toward the player, capped at 250 px/s.
- Attacking → Idle beyond `ORC_ATTACK_RANGE_LEAVE_PX` 160.
- Any state → Idle when horizontal speed falls below `ORC_MIN_MOVING_SPEED` 15 during a charge.

**Stopping safely** — a charge halts `ORC_WALL_STOP_DISTANCE_PX` 50 before a wall, and at a ledge: ground is probed `ORC_GROUND_PROBE_FEET_OFFSET_Y` 68 below the orc for at most `ORC_GROUND_PROBE_MAX_DIST` 96, at a point `ORC_LEDGE_PROBE_FORWARD_PX` 52 ahead. An orc never charges off an edge, though it still falls if the ground under it disappears.

**Power** — `ORC_MAX_POWER` 100; each sword hit removes `ORC_SWORD_POWER_FRACTION` 0.25 of max, so four hits defeat it. A non-lethal hit freezes it for `ORC_HIT_FREEZE_SEC` 0.35 and knocks it back `ORC_SWORD_KNOCKBACK_X` 260 / `ORC_SWORD_KNOCKBACK_Y` 120, held `ORC_SWORD_KNOCKBACK_SECS` 0.3. Defeat adds `ORC_DEFEAT_SCORE` 100 to the score and removes the orc for the rest of the run.

**Club** — while Attacking, the orc swings on a 0.833 s cycle and its club deals damage only between 0.25 s and 0.5 s of each swing, and only when the player is within `ORC_CLUB_HIT_MAX_SEPARATION_X_PX` 112 and `ORC_CLUB_HIT_MAX_SEPARATION_Y_PX` 82. `ORC_REATTACK_DELAY_SEC` 0.45 separates consecutive swings.

## Player condition

Health starts at `INITIAL_HEALTH` 3. A club hit costs `ORC_CLUB_DAMAGE` 1, knocks the player back `ORC_CLUB_KNOCKBACK_X` 300 / `ORC_CLUB_KNOCKBACK_Y` 170, locks movement and attacks for `HURT_LOCKOUT_SECS` 0.45, and grants `PLAYER_CLUB_IFRAMES` 1.3 s during which further club damage is refused.

**Shield** on `L`: the player is invulnerable for `SHIELD_ACTIVE_SECS` 2.0, after which it is locked out for `SHIELD_COOLDOWN_SECS` 5.0 before it can be raised again. Club damage is refused while it is up.

**Death** — health reaching 0, or touching a death pit at any health — respawns the character at the play spawn point with full health and zero velocity, and resets the run: every defeated orc is restored to full power at its level position and every collected coin returns. The score resets with it.

## Coins

Touching a coin collects it: the coin disappears for the rest of the run and `COIN_PICKUP_POINTS` 40 is added to the score. Its pickup box is the entity's `size` scaled by `COIN_LEVEL_SIZE_SCALE` 0.7, centred on the entity.

## Death pits

Touching a death pit kills the player outright, regardless of health or i-frames. Its box is the entity's `size` scaled by `DEATH_PIT_SIZE_SCALE` 4.0.

## Editor surface

The play overlay shows, alongside the existing position / grounded / state fields: health out of 3, the score, coins collected out of the level's total, and the shield's state — ready, active with seconds remaining, or cooling down with seconds remaining.

`F2` toggles a combat debug view, off by default, which draws the live slash hitbox, each orc's `ORC_ATTACK_RANGE_PX` 105 range circle, and a marker while an orc's club is in its damaging window. Orcs, coins and pits are drawn as plain shapes; there are no sprites, animation, VFX or audio.

## Non-destructive guarantee

Combat changes only per-session play state. It never mutates the level's entities, never calls `save_state()`, and never moves `has_unsaved_changes()`. Killing every orc and collecting every coin, then stopping play, leaves the level exactly as it was, and re-entering play restores them all.

## Structure

Combat lives in the pure simulation layer with no egui `Context`/`Ui` dependency, advanced by the existing 1/120 s fixed timestep, so all of its timing is unit-testable headlessly. `main.rs` owns the key mapping, the overlay, the debug drawing and the debug toggle.

## Acceptance

- A1 — `orc_tool`, `coin_tool` and `death_trap_tool` spawn an orc, a coin and a pit; other bitmap names spawn nothing.
- A2 — `J`/`X` swing, with a 0.12 s buffer and a 0.28 s cooldown.
- A3 — Up / down-airborne / otherwise-forward direction selection; left-click never attacks.
- A4 — Blade live only within the derived windows.
- A5 — Hitbox extents hit and miss correctly and keep the swing's starting facing.
- A6 — Recoil 400 for 0.1 s, pogo to 160 with the air dash restored, one hit per orc per swing.
- A7 — The four-state AI with its 105 / 400 / 160 thresholds, 1.0 s wind-up and 250 px/s cap.
- A8 — Charges stop 50 px before walls and at ledges; orcs fall under gravity.
- A9 — Four hits to defeat; 0.35 s freeze; 260/120 knockback for 0.3 s; 100 score.
- A10 — Club damages only in 0.25–0.5 s of its swing and only within 112 × 82.
- A11 — 1 damage, 300/170 knockback, 0.45 s lockout, 1.3 s i-frames.
- A12 — Shield invulnerable 2.0 s, then 5.0 s lockout.
- A13 — Coin pickup box is size × 0.7 and scores 40.
- A14 — Death pits kill regardless of health.
- A15 — Death respawns with 3 health and resets orcs, coins and score.
- A16 — Overlay shows health, score, coins collected of total, and shield state.
- A17 — `F2` toggles the debug view; off by default.
- A18 — A session with kills and pickups leaves the level and unsaved-changes untouched, and re-entering restores everything.
- A19 — Identical combat outcomes at 60 Hz and 144 Hz.
- A20 — The help lists `J`/`X`, `L` and `F2`.
