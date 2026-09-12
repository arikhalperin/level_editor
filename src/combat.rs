//! Pure combat simulation for the editor's play mode: the katana, orcs, coins, death
//! pits, the shield and the player's health.
//!
//! Like [`crate::sim`] this has no egui `Context`/`Ui` and no editor types — the level
//! arrives as plain `(name, pos, size)` triples and everything advances on an explicit
//! `dt` — so every timing below is unit-testable headlessly.
//!
//! Behaviour mirrors `/Users/arikha/bevy_prince_platformer` using the constants in
//! [`crate::game_config`]. The game runs this on Bevy ECS with Rapier shape queries,
//! which cannot run inside egui, so this is an independent implementation of the same
//! numbers: close, not bit-identical.
//!
//! Coordinates are the editor's world space: top-left origin, Y increasing downward.

use egui::{Pos2, Rect, Vec2};

use crate::game_config as cfg;
use crate::sim::{capsule_poly_mtv, Simulation, World, FIXED_DT, MAX_STEPS_PER_FRAME};

/// Which way the blade swings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashDirection {
    Forward,
    Up,
    Down,
}

impl SlashDirection {
    /// Swing length and the window within it where the blade connects, both derived
    /// from the game's animation clip table (see [`crate::game_config`]).
    fn timing(self) -> (f32, f32, f32) {
        match self {
            SlashDirection::Forward => (
                cfg::SLASH_FORWARD_DURATION,
                cfg::SLASH_FORWARD_HIT_FROM,
                cfg::SLASH_FORWARD_HIT_TO,
            ),
            _ => (
                cfg::SLASH_VERTICAL_DURATION,
                cfg::SLASH_VERTICAL_HIT_FROM,
                cfg::SLASH_VERTICAL_HIT_TO,
            ),
        }
    }

    /// Hitbox centre and half-extents for a swing from `origin` facing `facing`.
    pub fn hitbox(self, origin: Pos2, facing: f32) -> Rect {
        let (centre, half) = match self {
            SlashDirection::Forward => (
                Pos2::new(
                    origin.x + facing * cfg::SLASH_FORWARD_REACH,
                    origin.y - cfg::SLASH_FORWARD_Y,
                ),
                Vec2::new(cfg::SLASH_FORWARD_HALF_W, cfg::SLASH_FORWARD_HALF_H),
            ),
            SlashDirection::Up => (
                Pos2::new(origin.x, origin.y - cfg::SLASH_VERTICAL_REACH),
                Vec2::new(cfg::SLASH_VERTICAL_HALF_W, cfg::SLASH_VERTICAL_HALF_H),
            ),
            SlashDirection::Down => (
                Pos2::new(origin.x, origin.y + cfg::SLASH_VERTICAL_REACH),
                Vec2::new(cfg::SLASH_VERTICAL_HALF_W, cfg::SLASH_VERTICAL_HALF_H),
            ),
        };
        Rect::from_center_size(centre, half * 2.0)
    }
}

/// A swing in progress.
#[derive(Debug, Clone)]
pub struct ActiveSlash {
    pub direction: SlashDirection,
    pub elapsed: f32,
    duration: f32,
    hit_from: f32,
    hit_to: f32,
    /// Facing at swing start; the hitbox does not turn mid-swing.
    pub facing: f32,
    hit_orcs: Vec<usize>,
    pogoed: bool,
}

impl ActiveSlash {
    /// True while the blade connects.
    pub fn is_live(&self) -> bool {
        self.elapsed >= self.hit_from && self.elapsed <= self.hit_to
    }
}

/// Katana cooldown, press buffer and the current swing.
#[derive(Debug, Clone, Default)]
pub struct AttackState {
    pub slash: Option<ActiveSlash>,
    cooldown: f32,
    buffer: f32,
    was_held: bool,
}

/// The orc's AI state, as in the game's `OrcState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrcState {
    Idle,
    PreparingCharge,
    Charging,
    Attacking,
}

impl OrcState {
    pub fn label(self) -> &'static str {
        match self {
            OrcState::Idle => "idle",
            OrcState::PreparingCharge => "wind-up",
            OrcState::Charging => "charging",
            OrcState::Attacking => "attacking",
        }
    }
}

/// One orc. Simulated with the same capsule collision, gravity and terminal velocity as
/// the player, so it stands on exactly the level geometry the player does.
#[derive(Debug, Clone)]
pub struct Orc {
    /// Where it is restored to when the run resets.
    pub home: Pos2,
    pub pos: Pos2,
    pub vel: Vec2,
    pub state: OrcState,
    pub power: f32,
    pub alive: bool,
    pub facing: f32,
    pub grounded: bool,
    prepare_left: f32,
    freeze_left: f32,
    knockback_left: f32,
    /// Time into the current club swing; only meaningful while `Attacking`.
    pub swing_elapsed: f32,
    reattack_left: f32,
    /// Time left in which a sword freeze still counts as "already in the fight", so the
    /// next charge skips its wind-up. Lapses like the game's `OrcFreezeRecovered` timer
    /// rather than persisting forever.
    freeze_recovered_left: f32,
}

impl Orc {
    pub fn new(home: Pos2) -> Self {
        Self {
            home,
            pos: home,
            vel: Vec2::ZERO,
            state: OrcState::Idle,
            power: cfg::ORC_MAX_POWER,
            alive: true,
            facing: 1.0,
            grounded: false,
            prepare_left: 0.0,
            freeze_left: 0.0,
            knockback_left: 0.0,
            swing_elapsed: 0.0,
            reattack_left: 0.0,
            freeze_recovered_left: 0.0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.home);
    }

    /// True while this swing's club can damage the player.
    pub fn club_active(&self) -> bool {
        self.state == OrcState::Attacking
            && self.swing_elapsed >= cfg::ORC_CLUB_ACTIVE_FROM
            && self.swing_elapsed <= cfg::ORC_CLUB_ACTIVE_TO
    }

    /// Remaining power as a 0..1 fraction, for the debug bar.
    pub fn power_fraction(&self) -> f32 {
        (self.power / cfg::ORC_MAX_POWER).clamp(0.0, 1.0)
    }

    pub fn aabb(&self) -> Rect {
        Rect::from_center_size(
            self.pos,
            Vec2::new(
                cfg::ORC_CAPSULE_RADIUS * 2.0,
                (cfg::ORC_CAPSULE_HALF_HEIGHT + cfg::ORC_CAPSULE_RADIUS) * 2.0,
            ),
        )
    }
}

/// A collectible coin.
#[derive(Debug, Clone)]
pub struct Coin {
    pub rect: Rect,
    pub collected: bool,
}

/// A hazard that kills on contact.
#[derive(Debug, Clone)]
pub struct DeathPit {
    pub rect: Rect,
}

/// The timed invulnerability on `L` and its lockout.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shield {
    active_left: f32,
    cooldown_left: f32,
    was_held: bool,
}

impl Shield {
    pub fn is_active(&self) -> bool {
        self.active_left > 0.0
    }
    pub fn is_ready(&self) -> bool {
        self.active_left <= 0.0 && self.cooldown_left <= 0.0
    }
    pub fn active_left(&self) -> f32 {
        self.active_left
    }
    pub fn cooldown_left(&self) -> f32 {
        self.cooldown_left
    }
}

/// One level bitmap entity, in the plain form the combat layer understands.
#[derive(Debug, Clone)]
pub struct BitmapSpawn {
    pub name: String,
    /// Top-left corner, as the editor stores and draws it.
    pub pos: Pos2,
    pub size: Vec2,
}

/// Buttons the combat layer reads, on top of the movement input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatInput {
    pub attack: bool,
    pub shield: bool,
    pub up: bool,
    pub down: bool,
}

/// What a step produced, for the caller to react to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub died: bool,
    pub orc_defeated: bool,
    pub coin_collected: bool,
    pub player_hurt: bool,
}

/// Everything in a run that is not the player's movement.
#[derive(Debug, Clone, Default)]
pub struct Combat {
    pub orcs: Vec<Orc>,
    pub coins: Vec<Coin>,
    pub pits: Vec<DeathPit>,
    pub attack: AttackState,
    pub shield: Shield,
    pub health: i32,
    pub score: i32,
    iframes_left: f32,
}

impl Combat {
    /// Build a run from the level's bitmap entities, mapping names exactly as the game's
    /// importer does. An unrecognised name contributes nothing.
    ///
    /// The editor stores a bitmap's `pos` as its top-left corner, so each body is placed
    /// at the centre of the rectangle the user sees.
    pub fn from_bitmaps(bitmaps: &[BitmapSpawn]) -> Self {
        let mut c = Self {
            health: cfg::INITIAL_HEALTH,
            ..Default::default()
        };
        for b in bitmaps {
            let centre = b.pos + b.size * 0.5;
            match b.name.as_str() {
                cfg::ORC_BITMAP => c.orcs.push(Orc::new(centre)),
                cfg::COIN_BITMAP => c.coins.push(Coin {
                    rect: Rect::from_center_size(centre, b.size * cfg::COIN_LEVEL_SIZE_SCALE),
                    collected: false,
                }),
                cfg::DEATH_PIT_BITMAP => c.pits.push(DeathPit {
                    rect: Rect::from_center_size(centre, b.size * cfg::DEATH_PIT_SIZE_SCALE),
                }),
                _ => {}
            }
        }
        c
    }

    /// Restore the run: every orc back, every coin uncollected, health and score reset.
    pub fn reset(&mut self) {
        for orc in &mut self.orcs {
            orc.reset();
        }
        for coin in &mut self.coins {
            coin.collected = false;
        }
        self.attack = AttackState::default();
        self.shield = Shield::default();
        self.health = cfg::INITIAL_HEALTH;
        self.score = 0;
        self.iframes_left = 0.0;
    }

    pub fn coins_collected(&self) -> usize {
        self.coins.iter().filter(|c| c.collected).count()
    }

    pub fn orcs_alive(&self) -> usize {
        self.orcs.iter().filter(|o| o.alive).count()
    }

    /// True while a club hit would be refused.
    pub fn is_invulnerable(&self) -> bool {
        self.iframes_left > 0.0 || self.shield.is_active()
    }

    /// One fixed step, run after the player's own movement step.
    pub fn step(
        &mut self,
        player: &mut Simulation,
        world: &World,
        input: CombatInput,
        dt: f32,
    ) -> StepOutcome {
        let mut out = StepOutcome::default();

        self.iframes_left = (self.iframes_left - dt).max(0.0);
        self.step_shield(input, dt);
        self.step_attack(player, input, dt);
        out.orc_defeated = self.resolve_slash_hits(player);
        self.step_orcs(player, world, dt);
        out.player_hurt = self.resolve_club_hits(player);
        out.coin_collected = self.resolve_coins(player);

        if self.health <= 0 || self.touches_pit(player) {
            out.died = true;
        }
        out
    }

    fn step_shield(&mut self, input: CombatInput, dt: f32) {
        let was_active = self.shield.active_left > 0.0;
        self.shield.active_left = (self.shield.active_left - dt).max(0.0);
        self.shield.cooldown_left = (self.shield.cooldown_left - dt).max(0.0);
        if was_active && self.shield.active_left <= 0.0 {
            self.shield.cooldown_left = cfg::SHIELD_COOLDOWN_SECS;
        }
        if input.shield && !self.shield.was_held && self.shield.is_ready() {
            self.shield.active_left = cfg::SHIELD_ACTIVE_SECS;
        }
        self.shield.was_held = input.shield;
    }

    fn step_attack(&mut self, player: &mut Simulation, input: CombatInput, dt: f32) {
        self.attack.cooldown = (self.attack.cooldown - dt).max(0.0);
        self.attack.buffer = (self.attack.buffer - dt).max(0.0);
        if input.attack && !self.attack.was_held {
            self.attack.buffer = cfg::ATTACK_INPUT_BUFFER_SEC;
        }
        self.attack.was_held = input.attack;

        if let Some(slash) = self.attack.slash.as_mut() {
            slash.elapsed += dt;
            if slash.elapsed >= slash.duration {
                self.attack.slash = None;
            }
        }

        let can_attack = self.attack.slash.is_none()
            && self.attack.cooldown <= 0.0
            && !player.is_dashing()
            && !player.is_climbing()
            && !player.is_control_locked();
        if self.attack.buffer > 0.0 && can_attack {
            let direction = if input.up {
                SlashDirection::Up
            } else if input.down && !player.grounded {
                SlashDirection::Down
            } else {
                SlashDirection::Forward
            };
            let (duration, hit_from, hit_to) = direction.timing();
            self.attack.slash = Some(ActiveSlash {
                direction,
                elapsed: 0.0,
                duration,
                hit_from,
                hit_to,
                facing: player.facing,
                hit_orcs: Vec::new(),
                pogoed: false,
            });
            self.attack.cooldown = cfg::SLASH_COOLDOWN;
            self.attack.buffer = 0.0;
        }

        // The game multiplies the *run basis* speed during a ground swing, so the
        // equivalent here is a speed cap, not a per-step decay — multiplying the
        // velocity every fixed step would compound to a standstill. A knockback in
        // progress is exempt: recoil must not be clamped away.
        if self.attack.slash.is_some() && player.grounded && !player.is_input_locked() {
            let cap = cfg::PLAYER_RUN_SPEED * cfg::SLASH_GROUND_SPEED_MULT;
            player.vel.x = player.vel.x.clamp(-cap, cap);
        }
    }

    /// Returns true if a hit defeated an orc.
    fn resolve_slash_hits(&mut self, player: &mut Simulation) -> bool {
        let Some(slash) = self.attack.slash.as_mut() else {
            return false;
        };
        if !slash.is_live() {
            return false;
        }
        let box_ = slash.direction.hitbox(player.pos, slash.facing);
        let mut defeated = false;
        let airborne_down = slash.direction == SlashDirection::Down && !player.grounded;

        for (i, orc) in self.orcs.iter_mut().enumerate() {
            if !orc.alive || slash.hit_orcs.contains(&i) || !box_.intersects(orc.aabb()) {
                continue;
            }
            slash.hit_orcs.push(i);

            orc.power -= cfg::ORC_MAX_POWER * cfg::ORC_SWORD_POWER_FRACTION;
            let away = if orc.pos.x >= player.pos.x { 1.0 } else { -1.0 };
            if orc.power <= 0.0 {
                orc.alive = false;
                self.score += cfg::ORC_DEFEAT_SCORE;
                defeated = true;
            } else {
                orc.freeze_left = cfg::ORC_HIT_FREEZE_SEC;
                orc.knockback_left = cfg::ORC_SWORD_KNOCKBACK_SECS;
                orc.vel = Vec2::new(
                    away * cfg::ORC_SWORD_KNOCKBACK_X,
                    -cfg::ORC_SWORD_KNOCKBACK_Y,
                );
            }

            if slash.direction == SlashDirection::Forward {
                player.apply_knockback(
                    Vec2::new(-away * cfg::SLASH_RECOIL_SPEED, player.vel.y),
                    cfg::SLASH_RECOIL_SECS,
                );
            } else if airborne_down && !slash.pogoed {
                slash.pogoed = true;
                player.pogo(cfg::POGO_HEIGHT);
            }
        }
        defeated
    }

    fn step_orcs(&mut self, player: &Simulation, world: &World, dt: f32) {
        for orc in &mut self.orcs {
            if !orc.alive {
                continue;
            }
            step_one_orc(orc, player, world, dt);
        }
    }

    /// Returns true if the player took a club hit.
    fn resolve_club_hits(&mut self, player: &mut Simulation) -> bool {
        if self.is_invulnerable() {
            return false;
        }
        let mut hit_from: Option<f32> = None;
        for orc in &self.orcs {
            if !orc.alive || !orc.club_active() {
                continue;
            }
            let d = orc.pos - player.pos;
            if d.x.abs() <= cfg::ORC_CLUB_HIT_MAX_SEPARATION_X_PX
                && d.y.abs() <= cfg::ORC_CLUB_HIT_MAX_SEPARATION_Y_PX
            {
                hit_from = Some(orc.pos.x);
                break;
            }
        }
        let Some(orc_x) = hit_from else { return false };

        self.health -= cfg::ORC_CLUB_DAMAGE;
        self.iframes_left = cfg::PLAYER_CLUB_IFRAMES;
        let away = if player.pos.x >= orc_x { 1.0 } else { -1.0 };
        player.apply_knockback(
            Vec2::new(away * cfg::ORC_CLUB_KNOCKBACK_X, -cfg::ORC_CLUB_KNOCKBACK_Y),
            cfg::HURT_LOCKOUT_SECS,
        );
        player.lock_control(cfg::HURT_LOCKOUT_SECS);
        true
    }

    fn resolve_coins(&mut self, player: &Simulation) -> bool {
        let body = player.aabb();
        let mut got = false;
        for coin in &mut self.coins {
            if !coin.collected && body.intersects(coin.rect) {
                coin.collected = true;
                self.score += cfg::COIN_PICKUP_POINTS;
                got = true;
            }
        }
        got
    }

    fn touches_pit(&self, player: &Simulation) -> bool {
        let body = player.aabb();
        self.pits.iter().any(|p| body.intersects(p.rect))
    }
}

/// One orc's AI and motion for a fixed step.
fn step_one_orc(orc: &mut Orc, player: &Simulation, world: &World, dt: f32) {
    let was_frozen = orc.freeze_left > 0.0;
    orc.freeze_left = (orc.freeze_left - dt).max(0.0);
    orc.knockback_left = (orc.knockback_left - dt).max(0.0);
    orc.reattack_left = (orc.reattack_left - dt).max(0.0);
    orc.freeze_recovered_left = (orc.freeze_recovered_left - dt).max(0.0);

    let frozen = orc.freeze_left > 0.0;
    if was_frozen && !frozen {
        // The freeze just ended; the orc is briefly "already in the fight".
        orc.freeze_recovered_left = cfg::ORC_FREEZE_RECOVERED_SECS;
    }

    let to_player = player.pos - orc.pos;
    let distance = to_player.length();
    let dir_x = if to_player.x >= 0.0 { 1.0 } else { -1.0 };

    if !frozen && orc.knockback_left <= 0.0 {
        // ── Transitions (game: `handle_orc_detection`) ──
        match orc.state {
            OrcState::Idle => {
                if distance < cfg::ORC_ATTACK_RANGE_PX {
                    if orc.reattack_left <= 0.0 {
                        orc.state = OrcState::Attacking;
                        orc.swing_elapsed = 0.0;
                    }
                } else if distance < cfg::ORC_CHARGE_TRIGGER_PX {
                    orc.state = OrcState::PreparingCharge;
                    // Coming out of a sword freeze the orc is already in the fight and
                    // skips the wind-up, as the game does.
                    orc.prepare_left = if orc.freeze_recovered_left > 0.0 {
                        0.0
                    } else {
                        cfg::ORC_PREPARE_CHARGE_SECS
                    };
                    orc.freeze_recovered_left = 0.0;
                }
            }
            OrcState::PreparingCharge => {
                orc.facing = dir_x;
                orc.vel.x = 0.0;
                orc.prepare_left = (orc.prepare_left - dt).max(0.0);
                if orc.prepare_left <= 0.0 {
                    orc.state = OrcState::Charging;
                    orc.vel.x = dir_x * cfg::ORC_CHARGE_LAUNCH_SPEED;
                }
            }
            OrcState::Charging => {
                orc.facing = dir_x;
                if distance < cfg::ORC_ATTACK_RANGE_PX && orc.reattack_left <= 0.0 {
                    orc.state = OrcState::Attacking;
                    orc.swing_elapsed = 0.0;
                    orc.vel.x = 0.0;
                } else if blocked_ahead(orc, world) || !ground_ahead(orc, world) {
                    orc.state = OrcState::Idle;
                    orc.vel.x = 0.0;
                } else if orc.vel.x.abs() < cfg::ORC_MIN_MOVING_SPEED {
                    // Checked before accelerating, as the game insists: otherwise the
                    // boost re-lifts a stalled orc above the threshold every step and it
                    // never gives up (e.g. shoving against a wall).
                    orc.state = OrcState::Idle;
                    orc.vel.x = 0.0;
                } else {
                    orc.vel.x += dir_x * cfg::ORC_CHARGE_ACCEL * dt;
                    orc.vel.x = orc
                        .vel
                        .x
                        .clamp(-cfg::ORC_CHARGE_MAX_SPEED, cfg::ORC_CHARGE_MAX_SPEED);
                }
            }
            OrcState::Attacking => {
                orc.facing = dir_x;
                orc.vel.x = 0.0;
                if orc.reattack_left > 0.0 {
                    // Between swings: the club is down for ORC_REATTACK_DELAY_SEC.
                    orc.swing_elapsed = 0.0;
                } else {
                    orc.swing_elapsed += dt;
                    if orc.swing_elapsed >= cfg::ORC_SWING_DURATION {
                        orc.swing_elapsed = 0.0;
                        orc.reattack_left = cfg::ORC_REATTACK_DELAY_SEC;
                    }
                }
                if distance > cfg::ORC_ATTACK_RANGE_LEAVE_PX {
                    orc.state = OrcState::Idle;
                    orc.reattack_left = orc.reattack_left.max(cfg::ORC_REATTACK_DELAY_SEC);
                }
            }
        }
    } else {
        // Frozen or being knocked back: no decisions, but gravity and collision run.
        // The knockback keeps its velocity even while the freeze is running, as the
        // game's `OrcKnockback` does; only once it expires does the freeze hold the
        // orc still.
        if frozen && orc.knockback_left <= 0.0 {
            orc.vel.x = 0.0;
        }
    }

    // ── Gravity and motion, matching the player's ──
    orc.vel.y += cfg::GRAVITY * dt;
    if orc.vel.y > cfg::MAX_FALL_SPEED {
        orc.vel.y = cfg::MAX_FALL_SPEED;
    }
    let grounded = move_capsule(
        &mut orc.pos,
        &mut orc.vel,
        cfg::ORC_CAPSULE_HALF_HEIGHT,
        cfg::ORC_CAPSULE_RADIUS,
        world,
        dt,
    );
    orc.grounded = grounded;
}

/// True when a wall is within [`cfg::ORC_WALL_STOP_DISTANCE_PX`] ahead of the orc.
fn blocked_ahead(orc: &Orc, world: &World) -> bool {
    let probe = Pos2::new(
        orc.pos.x + orc.facing * cfg::ORC_WALL_STOP_DISTANCE_PX,
        orc.pos.y,
    );
    point_in_world(world, probe)
}

/// True when there is ground under the point the orc's feet are about to reach.
///
/// The game box-casts down from the feet; this samples the same ray, which is enough at
/// the 4 px resolution used here and keeps the module free of a physics engine.
fn ground_ahead(orc: &Orc, world: &World) -> bool {
    let x = orc.pos.x + orc.facing * cfg::ORC_LEDGE_PROBE_FORWARD_PX;
    let top = orc.pos.y + cfg::ORC_GROUND_PROBE_FEET_OFFSET_Y;
    let mut d = 0.0;
    while d <= cfg::ORC_GROUND_PROBE_MAX_DIST {
        if point_in_world(world, Pos2::new(x, top + d)) {
            return true;
        }
        d += 4.0;
    }
    false
}

fn point_in_world(world: &World, p: Pos2) -> bool {
    world.polys.iter().any(|poly| point_in_convex(&poly.points, p))
}

fn point_in_convex(points: &[Pos2], p: Pos2) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut sign = 0.0_f32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Move a capsule by its velocity and resolve overlap, returning whether it ended up on
/// the ground. Same scheme as the player's integrator, so orcs stand on exactly the
/// geometry the player does.
fn move_capsule(
    pos: &mut Pos2,
    vel: &mut Vec2,
    half_height: f32,
    radius: f32,
    world: &World,
    dt: f32,
) -> bool {
    let travel = (*vel * dt).length();
    let sub = ((travel / (radius * 0.25)).ceil() as u32).clamp(1, 16);
    let sdt = dt / sub as f32;
    let mut grounded = false;

    for _ in 0..sub {
        *pos += *vel * sdt;
        for _ in 0..4 {
            let mut deepest: Option<Vec2> = None;
            for poly in &world.polys {
                if let Some(mtv) = capsule_poly_mtv(*pos, half_height, radius, &poly.points) {
                    if mtv.length() > deepest.map_or(1e-4, |d: Vec2| d.length()) {
                        deepest = Some(mtv);
                    }
                }
            }
            let Some(mtv) = deepest else { break };
            *pos += mtv;
            let n = mtv.normalized();
            if n.y < -0.5 {
                grounded = true;
                if vel.y > 0.0 {
                    vel.y = 0.0;
                }
            } else if n.y > 0.5 {
                if vel.y < 0.0 {
                    vel.y = 0.0;
                }
            } else if vel.x.signum() == -n.x.signum() {
                vel.x = 0.0;
            }
        }
    }

    if !grounded && vel.y >= 0.0 {
        let probe = Pos2::new(pos.x, pos.y + 2.0);
        grounded = world.polys.iter().any(|poly| {
            capsule_poly_mtv(probe, half_height, radius, &poly.points)
                .is_some_and(|m| m.normalized().y < -0.5)
        });
    }
    grounded
}

/// Movement plus combat input for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayInput {
    pub sim: crate::sim::Input,
    pub attack: bool,
    pub shield: bool,
}

/// The player and the run, advanced together on one fixed-step accumulator so combat
/// timing is as frame-rate independent as movement.
#[derive(Debug, Clone)]
pub struct PlaySession {
    pub player: Simulation,
    pub combat: Combat,
    accumulator: f32,
    /// Set for one frame after a death, so the caller can react.
    pub just_died: bool,
}

impl PlaySession {
    pub fn new(spawn: Pos2, bitmaps: &[BitmapSpawn]) -> Self {
        Self {
            player: Simulation::new(spawn),
            combat: Combat::from_bitmaps(bitmaps),
            accumulator: 0.0,
            just_died: false,
        }
    }

    /// Restart the run: the character returns to the spawn point and every orc, coin,
    /// health point and score is restored.
    pub fn restart(&mut self) {
        self.player.respawn();
        self.combat.reset();
    }

    /// Move the spawn point and restart there.
    pub fn respawn_at(&mut self, spawn: Pos2) {
        self.player.respawn_at(spawn);
        self.combat.reset();
    }

    /// Advance real time in whole fixed steps. Returns the number of steps taken.
    pub fn advance(&mut self, world: &World, input: PlayInput, dt: f32) -> u32 {
        self.just_died = false;
        if !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.accumulator += dt;
        let mut steps = 0;
        while self.accumulator >= FIXED_DT && steps < MAX_STEPS_PER_FRAME {
            self.step(world, input, FIXED_DT);
            self.accumulator -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_STEPS_PER_FRAME {
            self.accumulator = 0.0;
        }
        steps
    }

    /// One fixed step: movement first, then combat against the moved positions.
    pub fn step(&mut self, world: &World, input: PlayInput, dt: f32) {
        self.player.step(world, input.sim, dt);
        let combat_input = CombatInput {
            attack: input.attack,
            shield: input.shield,
            up: input.sim.up,
            down: input.sim.down,
        };
        let out = self.combat.step(&mut self.player, world, combat_input, dt);
        if out.died {
            self.restart();
            self.just_died = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{CollisionPoly, Input};

    const DT: f32 = FIXED_DT;

    fn rect_poly(x0: f32, y0: f32, x1: f32, y1: f32) -> CollisionPoly {
        CollisionPoly::new(
            &[
                Pos2::new(x0, y0),
                Pos2::new(x1, y0),
                Pos2::new(x1, y1),
                Pos2::new(x0, y1),
            ],
            false,
        )
        .expect("valid rect")
    }

    /// Wide floor with its surface at y = 500.
    fn floor() -> World {
        World { polys: vec![rect_poly(-5000.0, 500.0, 5000.0, 900.0)] }
    }

    fn bitmap(name: &str, x: f32, y: f32, w: f32, h: f32) -> BitmapSpawn {
        BitmapSpawn { name: name.to_string(), pos: Pos2::new(x, y), size: Vec2::new(w, h) }
    }

    /// A session standing on the floor, with whatever bitmaps are given.
    fn standing(bitmaps: &[BitmapSpawn]) -> (PlaySession, World) {
        let w = floor();
        let mut s = PlaySession::new(Pos2::new(0.0, 400.0), bitmaps);
        run(&mut s, &w, PlayInput::default(), 1.0);
        assert!(s.player.grounded, "should have landed");
        (s, w)
    }

    fn run(s: &mut PlaySession, w: &World, input: PlayInput, secs: f32) {
        for _ in 0..((secs / DT).round() as usize) {
            s.step(w, input, DT);
        }
    }

    fn attack_input() -> PlayInput {
        PlayInput { attack: true, ..Default::default() }
    }

    /// Player resting on the floor, plus a stationary orc `dx` ahead at its own rest
    /// height. The orc is added *after* the player settles, so its AI cannot club the
    /// player during setup, and `iframes_left` is pinned so only the swing is measured.
    fn duel(dx: f32) -> (PlaySession, World) {
        let (mut s, w) = standing(&[]);
        s.combat.orcs.push(Orc::new(Pos2::new(dx, ORC_REST_Y)));
        s.combat.iframes_left = 1.0e6;
        (s, w)
    }

    /// Where an orc capsule comes to rest on the floor at y = 500.
    const ORC_REST_Y: f32 = 500.0 - (cfg::ORC_CAPSULE_HALF_HEIGHT + cfg::ORC_CAPSULE_RADIUS);
    /// Where the player capsule comes to rest on the same floor.
    const PLAYER_REST_Y: f32 = 500.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y;

    // ── A1: level mapping ───────────────────────────────────────────────────────

    #[test]
    fn bitmap_names_map_to_the_same_things_the_game_spawns() {
        let c = Combat::from_bitmaps(&[
            bitmap("orc_tool", 100.0, 100.0, 128.0, 128.0),
            bitmap("coin_tool", 300.0, 100.0, 64.0, 64.0),
            bitmap("death_trap_tool", 500.0, 100.0, 48.0, 48.0),
            bitmap("wall_tool", 700.0, 100.0, 78.0, 128.0),
            bitmap("anything_else", 900.0, 100.0, 32.0, 32.0),
        ]);
        assert_eq!(c.orcs.len(), 1);
        assert_eq!(c.coins.len(), 1);
        assert_eq!(c.pits.len(), 1);
        // Bodies sit at the centre of the rectangle the editor draws.
        assert_eq!(c.orcs[0].home, Pos2::new(164.0, 164.0));
        assert_eq!(c.health, cfg::INITIAL_HEALTH);
        assert_eq!(c.score, 0);
    }

    #[test]
    fn pickup_and_hazard_boxes_use_the_games_scales() {
        let c = Combat::from_bitmaps(&[
            bitmap("coin_tool", 0.0, 0.0, 100.0, 100.0),
            bitmap("death_trap_tool", 0.0, 0.0, 100.0, 100.0),
        ]);
        assert_eq!(c.coins[0].rect.width(), 100.0 * cfg::COIN_LEVEL_SIZE_SCALE);
        assert_eq!(c.pits[0].rect.width(), 100.0 * cfg::DEATH_PIT_SIZE_SCALE);
    }

    #[test]
    fn an_empty_level_yields_an_empty_run() {
        let c = Combat::from_bitmaps(&[]);
        assert!(c.orcs.is_empty() && c.coins.is_empty() && c.pits.is_empty());
    }

    // ── A2, A3: attack input ────────────────────────────────────────────────────

    #[test]
    fn attack_starts_a_swing_and_respects_the_cooldown() {
        let (mut s, w) = standing(&[]);
        s.step(&w, attack_input(), DT);
        assert!(s.combat.attack.slash.is_some(), "J should swing");

        run(&mut s, &w, PlayInput::default(), cfg::SLASH_FORWARD_DURATION);
        assert!(s.combat.attack.slash.is_none(), "swing should have ended");
        s.step(&w, attack_input(), DT);
        assert!(
            s.combat.attack.slash.is_none(),
            "a second swing inside {}s must be refused",
            cfg::SLASH_COOLDOWN
        );

        // That refused press stays buffered for ATTACK_INPUT_BUFFER_SEC and fires as soon
        // as the cooldown clears, which starts a fresh cooldown; wait the whole cycle out.
        run(
            &mut s,
            &w,
            PlayInput::default(),
            cfg::ATTACK_INPUT_BUFFER_SEC + cfg::SLASH_FORWARD_DURATION + cfg::SLASH_COOLDOWN,
        );
        assert!(s.combat.attack.slash.is_none());
        s.step(&w, attack_input(), DT);
        assert!(s.combat.attack.slash.is_some(), "allowed once fully off cooldown");
    }

    #[test]
    fn an_attack_press_is_buffered_until_the_cooldown_clears() {
        let (mut s, w) = standing(&[]);
        s.step(&w, attack_input(), DT);
        run(&mut s, &w, PlayInput::default(), cfg::SLASH_FORWARD_DURATION);
        assert!(s.combat.attack.cooldown > 0.0, "still cooling down");

        // One press inside the cooldown, within the buffer window of its end.
        let wait = (cfg::SLASH_COOLDOWN
            - cfg::SLASH_FORWARD_DURATION
            - cfg::ATTACK_INPUT_BUFFER_SEC * 0.5)
            .max(0.0);
        run(&mut s, &w, PlayInput::default(), wait);
        s.step(&w, attack_input(), DT);
        let mut fired = false;
        for _ in 0..((cfg::ATTACK_INPUT_BUFFER_SEC / DT).ceil() as usize + 2) {
            s.step(&w, PlayInput::default(), DT);
            if s.combat.attack.slash.is_some() {
                fired = true;
                break;
            }
        }
        assert!(fired, "a buffered press must fire when the cooldown clears");
    }

    #[test]
    fn slash_direction_follows_the_held_keys() {
        let (mut s, w) = standing(&[]);
        let up = PlayInput { attack: true, sim: Input { up: true, ..Input::default() }, ..Default::default() };
        s.step(&w, up, DT);
        assert_eq!(s.combat.attack.slash.as_ref().unwrap().direction, SlashDirection::Up);

        // Grounded + down is still a forward slash; only airborne gives a down slash.
        let (mut s, w) = standing(&[]);
        let down = PlayInput { attack: true, sim: Input { down: true, ..Input::default() }, ..Default::default() };
        s.step(&w, down, DT);
        assert_eq!(
            s.combat.attack.slash.as_ref().unwrap().direction,
            SlashDirection::Forward,
            "down only slashes down while airborne"
        );

        let (mut s, w) = standing(&[]);
        run(&mut s, &w, PlayInput { sim: Input { jump: true, ..Input::default() }, ..Default::default() }, 0.1);
        assert!(!s.player.grounded);
        s.step(&w, down, DT);
        assert_eq!(s.combat.attack.slash.as_ref().unwrap().direction, SlashDirection::Down);
    }

    #[test]
    fn slashing_is_refused_while_dashing() {
        let (mut s, w) = standing(&[]);
        let dash = PlayInput {
            sim: Input { right: true, dash: true, ..Input::default() },
            attack: true,
            ..Default::default()
        };
        s.step(&w, dash, DT);
        assert!(s.player.is_dashing());
        assert!(s.combat.attack.slash.is_none(), "no swinging mid-dash");
    }

    // ── A4, A5: slash timing and reach ──────────────────────────────────────────

    #[test]
    fn slash_hit_windows_match_the_games_clip_table() {
        assert_eq!(SlashDirection::Forward.timing(), (0.25, 5.0 / 60.0, 9.0 / 60.0));
        assert_eq!(SlashDirection::Up.timing(), (0.25, 2.0 / 32.0, 6.0 / 32.0));
        assert_eq!(SlashDirection::Down.timing(), SlashDirection::Up.timing());
    }

    #[test]
    fn the_blade_only_connects_inside_its_window() {
        let (mut s, w) = duel(54.0);
        let full = cfg::ORC_MAX_POWER;
        s.step(&w, attack_input(), DT);
        assert!(s.combat.attack.slash.as_ref().unwrap().elapsed < cfg::SLASH_FORWARD_HIT_FROM);
        assert_eq!(s.combat.orcs[0].power, full, "no damage before the window opens");

        run(&mut s, &w, PlayInput::default(), cfg::SLASH_FORWARD_HIT_TO);
        assert!(s.combat.orcs[0].power < full, "the orc should be hit inside the window");
    }

    #[test]
    fn an_orc_outside_the_hitbox_is_not_hit() {
        // Far beyond SLASH_FORWARD_REACH + SLASH_FORWARD_HALF_W, and beyond its own
        // charge trigger so it stays put.
        let (mut s, w) = duel(cfg::ORC_CHARGE_TRIGGER_PX + 200.0);
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert_eq!(s.combat.orcs[0].power, cfg::ORC_MAX_POWER);
    }

    #[test]
    fn a_swing_keeps_hitting_where_it_started_even_if_the_player_turns() {
        // Orc ahead on the right; start the swing facing right, then hold left for the
        // rest of it. The blade must still connect on the right.
        let (mut s, w) = duel(54.0);
        s.combat.orcs[0].state = OrcState::Attacking; // hold it still
        assert_eq!(s.player.facing, 1.0);
        s.step(&w, attack_input(), DT);
        let turn_left = PlayInput { sim: Input { left: true, ..Input::default() }, ..Default::default() };
        run(&mut s, &w, turn_left, cfg::SLASH_FORWARD_DURATION);
        assert_eq!(s.player.facing, -1.0, "the player did turn");
        assert!(
            s.combat.orcs[0].power < cfg::ORC_MAX_POWER,
            "the swing must keep the facing it began with"
        );
    }

    #[test]
    fn hitbox_geometry_mirrors_with_facing() {
        let origin = Pos2::new(0.0, 0.0);
        let right = SlashDirection::Forward.hitbox(origin, 1.0);
        let left = SlashDirection::Forward.hitbox(origin, -1.0);
        assert_eq!(right.center().x, cfg::SLASH_FORWARD_REACH);
        assert_eq!(left.center().x, -cfg::SLASH_FORWARD_REACH);
        assert_eq!(right.width(), cfg::SLASH_FORWARD_HALF_W * 2.0);
        assert_eq!(right.height(), cfg::SLASH_FORWARD_HALF_H * 2.0);

        let up = SlashDirection::Up.hitbox(origin, 1.0);
        let down = SlashDirection::Down.hitbox(origin, 1.0);
        assert_eq!(up.center().y, -cfg::SLASH_VERTICAL_REACH);
        assert_eq!(down.center().y, cfg::SLASH_VERTICAL_REACH);
    }

    #[test]
    fn an_orc_behind_the_player_is_not_hit_by_a_forward_slash() {
        let (mut s, w) = duel(-54.0);
        assert_eq!(s.player.facing, 1.0, "facing right by default");
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert_eq!(s.combat.orcs[0].power, cfg::ORC_MAX_POWER, "the blade swings forward only");
    }

    // ── A6, A9: hit effects ─────────────────────────────────────────────────────

    #[test]
    fn four_sword_hits_defeat_an_orc_and_score_the_defeat() {
        let (mut s, w) = duel(54.0);
        let mut swings = 0;
        while s.combat.orcs[0].alive && swings < 10 {
            // Recoil pushes the player away after each landed hit; step back in so the
            // next swing is measured from the same distance.
            s.player.pos = Pos2::new(0.0, PLAYER_REST_Y);
            s.combat.orcs[0].pos = Pos2::new(54.0, ORC_REST_Y);
            s.combat.iframes_left = 1.0e6;
            run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
            run(&mut s, &w, PlayInput::default(), cfg::SLASH_COOLDOWN + cfg::ATTACK_INPUT_BUFFER_SEC);
            swings += 1;
        }
        assert_eq!(swings, 4, "a quarter of max power per hit means four hits");
        assert!(!s.combat.orcs[0].alive);
        assert_eq!(s.combat.score, cfg::ORC_DEFEAT_SCORE);
        assert_eq!(s.combat.orcs_alive(), 0);
    }

    #[test]
    fn a_non_lethal_hit_freezes_and_knocks_the_orc_back() {
        let (mut s, w) = duel(54.0);
        // Step to the instant of the hit: gravity would otherwise have eaten the pop.
        let mut landed = false;
        for _ in 0..((cfg::SLASH_FORWARD_DURATION / DT).ceil() as usize + 2) {
            s.step(&w, attack_input(), DT);
            if s.combat.orcs[0].power < cfg::ORC_MAX_POWER {
                landed = true;
                break;
            }
        }
        assert!(landed, "the slash should have connected");
        let orc = &s.combat.orcs[0];
        assert!(orc.alive);
        assert!(
            orc.freeze_left > 0.0 && orc.freeze_left <= cfg::ORC_HIT_FREEZE_SEC,
            "freeze {} should be within {}",
            orc.freeze_left,
            cfg::ORC_HIT_FREEZE_SEC
        );
        assert!(
            (orc.vel.x - cfg::ORC_SWORD_KNOCKBACK_X).abs() < 1.0,
            "knocked away at exactly {} px/s, got {}",
            cfg::ORC_SWORD_KNOCKBACK_X,
            orc.vel.x
        );
        // The hit resolves before the orc integrates, so exactly one step of gravity has
        // been applied to the pop by the time it is observed.
        let one_step_of_gravity = cfg::GRAVITY * DT;
        assert!(
            (orc.vel.y + cfg::ORC_SWORD_KNOCKBACK_Y - one_step_of_gravity).abs() < 1.0,
            "and popped up at -{} (less one gravity step), got {}",
            cfg::ORC_SWORD_KNOCKBACK_Y,
            orc.vel.y
        );
        // The knockback is held rather than cancelled by the freeze that runs with it.
        run(&mut s, &w, PlayInput::default(), cfg::ORC_SWORD_KNOCKBACK_SECS * 0.5);
        assert!(
            s.combat.orcs[0].vel.x > cfg::ORC_SWORD_KNOCKBACK_X * 0.5,
            "the freeze must not cancel the knockback, vx {}",
            s.combat.orcs[0].vel.x
        );
    }

    #[test]
    fn the_club_misses_outside_its_separation_box() {
        let (mut s, w) = standing(&[]);
        // Orc attacking with its club live, but further than the club's X reach.
        let orc_x = cfg::ORC_CLUB_HIT_MAX_SEPARATION_X_PX + 10.0;
        s.combat.orcs.push(Orc::new(Pos2::new(orc_x, ORC_REST_Y)));
        let health = s.combat.health;
        for _ in 0..((cfg::ORC_SWING_DURATION * 2.0 / DT) as usize) {
            // Hold the orc attacking and in range of its own state machine.
            s.combat.orcs[0].pos = Pos2::new(orc_x, ORC_REST_Y);
            s.combat.orcs[0].state = OrcState::Attacking;
            s.step(&w, PlayInput::default(), DT);
        }
        assert_eq!(
            s.combat.health, health,
            "a club beyond {} px must not connect",
            cfg::ORC_CLUB_HIT_MAX_SEPARATION_X_PX
        );
    }

    #[test]
    fn one_orc_takes_at_most_one_hit_per_swing() {
        let (mut s, w) = duel(54.0);
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert_eq!(
            s.combat.orcs[0].power,
            cfg::ORC_MAX_POWER - cfg::ORC_MAX_POWER * cfg::ORC_SWORD_POWER_FRACTION,
            "the whole swing should have taken exactly one quarter"
        );
    }

    #[test]
    fn a_landed_forward_slash_recoils_the_player_away() {
        let (mut s, w) = duel(54.0);
        // Step until the hit lands so the recoil is read at the instant it is applied.
        let mut landed = false;
        for _ in 0..((cfg::SLASH_FORWARD_DURATION / DT).ceil() as usize + 2) {
            s.step(&w, attack_input(), DT);
            if s.combat.orcs[0].power < cfg::ORC_MAX_POWER {
                landed = true;
                break;
            }
        }
        assert!(landed, "the slash should have connected");
        assert!(
            (s.player.vel.x + cfg::SLASH_RECOIL_SPEED).abs() < 1.0,
            "recoil should be exactly -{} px/s away from the orc, got {}",
            cfg::SLASH_RECOIL_SPEED,
            s.player.vel.x
        );
        // And it is held against input for SLASH_RECOIL_SECS rather than braked away.
        run(&mut s, &w, PlayInput { sim: Input { right: true, ..Input::default() }, ..Default::default() }, cfg::SLASH_RECOIL_SECS * 0.5);
        assert!(
            s.player.vel.x < -cfg::SLASH_RECOIL_SPEED * 0.9,
            "the recoil must hold against input, vx {}",
            s.player.vel.x
        );
    }

    #[test]
    fn an_airborne_down_slash_pogos_and_restores_the_air_dash() {
        let (mut s, w) = standing(&[]);
        // Orc on the floor, player hanging directly above it within the down-slash box.
        s.combat.orcs.push(Orc::new(Pos2::new(0.0, ORC_REST_Y)));
        s.combat.iframes_left = 1.0e6;
        s.player.respawn_at(Pos2::new(0.0, ORC_REST_Y - cfg::SLASH_VERTICAL_REACH));

        // Spend the air dash first, then wait out its cooldown.
        s.step(&w, PlayInput { sim: Input { right: true, dash: true, ..Input::default() }, ..Default::default() }, DT);
        assert!(s.player.is_dashing());
        run(&mut s, &w, PlayInput::default(), cfg::DASH_DISTANCE / cfg::DASH_SPEED + cfg::DASH_COOLDOWN);
        // The dash and its cooldown drop the player back to the floor; lift it to the
        // slash position again and confirm it is airborne there.
        s.player.pos = Pos2::new(0.0, ORC_REST_Y - cfg::SLASH_VERTICAL_REACH);
        s.step(&w, PlayInput::default(), DT);
        s.player.pos = Pos2::new(0.0, ORC_REST_Y - cfg::SLASH_VERTICAL_REACH);
        assert!(!s.player.grounded, "must be airborne for a down slash");

        let down = Input { down: true, ..Input::default() };
        s.step(&w, PlayInput { attack: true, sim: down, ..Default::default() }, DT);
        assert_eq!(s.combat.attack.slash.as_ref().unwrap().direction, SlashDirection::Down);
        let mut pogoed = false;
        for _ in 0..((cfg::SLASH_VERTICAL_DURATION / DT).ceil() as usize + 2) {
            s.step(&w, PlayInput { sim: down, ..Default::default() }, DT);
            if s.player.vel.y < 0.0 {
                pogoed = true;
                break;
            }
        }
        assert!(pogoed, "pogo should send the player upward, vy {}", s.player.vel.y);
        let launch = -s.player.vel.y;
        let expected = (2.0 * cfg::GRAVITY * cfg::POGO_HEIGHT).sqrt();
        assert!(
            (launch - expected).abs() < 1.0,
            "pogo launch {launch} should reach POGO_HEIGHT {} (expected {expected} px/s)",
            cfg::POGO_HEIGHT
        );

        // The air dash is back even though the player never landed.
        s.step(&w, PlayInput { sim: Input { right: true, dash: true, ..Input::default() }, ..Default::default() }, DT);
        assert!(s.player.is_dashing(), "the pogo should have restored the air dash");
    }

    // ── A7, A8: orc AI ──────────────────────────────────────────────────────────

    /// Player settled on the floor at `player_x`, then one orc placed at the origin.
    fn orc_and_player(player_x: f32) -> (PlaySession, World) {
        let w = floor();
        let mut s = PlaySession::new(Pos2::new(player_x, 400.0), &[]);
        run(&mut s, &w, PlayInput::default(), 1.0);
        s.combat.orcs.push(Orc::new(Pos2::new(0.0, ORC_REST_Y)));
        (s, w)
    }

    #[test]
    fn an_orc_stays_idle_until_the_player_is_close_enough_to_charge() {
        let (mut s, w) = orc_and_player(cfg::ORC_CHARGE_TRIGGER_PX + 200.0);
        run(&mut s, &w, PlayInput::default(), 0.5);
        assert_eq!(s.combat.orcs[0].state, OrcState::Idle, "too far to react");
    }

    #[test]
    fn an_orc_winds_up_then_charges_and_is_speed_capped() {
        let (mut s, w) = orc_and_player(300.0);
        s.step(&w, PlayInput::default(), DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::PreparingCharge);

        run(&mut s, &w, PlayInput::default(), cfg::ORC_PREPARE_CHARGE_SECS - 4.0 * DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::PreparingCharge, "still winding up");
        assert_eq!(s.combat.orcs[0].vel.x, 0.0, "no movement during the wind-up");

        run(&mut s, &w, PlayInput::default(), 8.0 * DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::Charging);
        run(&mut s, &w, PlayInput::default(), 0.3);
        assert!(
            s.combat.orcs[0].vel.x.abs() <= cfg::ORC_CHARGE_MAX_SPEED + 1.0,
            "charge speed {} must be capped at {}",
            s.combat.orcs[0].vel.x,
            cfg::ORC_CHARGE_MAX_SPEED
        );
        assert!(s.combat.orcs[0].vel.x > 0.0, "charging toward the player");
    }

    #[test]
    fn an_orc_attacks_in_range_and_gives_up_beyond_the_leave_range() {
        let (mut s, w) = orc_and_player(cfg::ORC_ATTACK_RANGE_PX - 20.0);
        s.combat.iframes_left = 1.0e6;
        s.step(&w, PlayInput::default(), DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::Attacking);

        // Move the player well beyond the leave range.
        s.player.pos.x = cfg::ORC_ATTACK_RANGE_LEAVE_PX + 100.0;
        s.step(&w, PlayInput::default(), DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::Idle);
    }

    #[test]
    fn a_charging_orc_stops_at_a_ledge_instead_of_walking_off() {
        // The orc's ledge ends at x = 200; a second platform beyond the gap carries the
        // player, close enough (< ORC_CHARGE_TRIGGER_PX) that the orc really does charge.
        let w = World {
            polys: vec![
                rect_poly(-500.0, 500.0, 200.0, 900.0),
                rect_poly(300.0, 500.0, 900.0, 900.0),
            ],
        };
        let mut s = PlaySession::new(Pos2::new(350.0, 400.0), &[]);
        run(&mut s, &w, PlayInput::default(), 1.0);
        assert!(s.player.grounded, "the player must be standing, not falling out of range");
        s.combat.orcs.push(Orc::new(Pos2::new(0.0, ORC_REST_Y)));

        let start_x = s.combat.orcs[0].pos.x;
        let mut charged = false;
        for _ in 0..((4.0 / DT) as usize) {
            s.step(&w, PlayInput::default(), DT);
            if s.combat.orcs[0].state == OrcState::Charging {
                charged = true;
            }
        }
        let orc = &s.combat.orcs[0];
        assert!(charged, "the orc must actually charge for this to test anything");
        assert!(orc.pos.x > start_x + 20.0, "and must have travelled, x {}", orc.pos.x);
        // It stopped before its feet could cross the edge, not merely before falling off.
        assert!(
            orc.pos.x + cfg::ORC_LEDGE_PROBE_FORWARD_PX <= 200.0 + 8.0,
            "the orc's ledge probe crossed the edge: x {} + {} > 200",
            orc.pos.x,
            cfg::ORC_LEDGE_PROBE_FORWARD_PX
        );
        assert!(orc.grounded, "and it is still standing on the ledge");
        assert!(orc.pos.y < 600.0, "it must not have fallen into the gap, y {}", orc.pos.y);
    }

    #[test]
    fn a_charging_orc_stops_before_a_wall() {
        // Wall rising from the floor at x = 200; the player is on the far side.
        let w = World {
            polys: vec![
                rect_poly(-500.0, 500.0, 900.0, 900.0),
                rect_poly(200.0, 100.0, 260.0, 500.0),
            ],
        };
        let mut s = PlaySession::new(Pos2::new(350.0, 400.0), &[]);
        run(&mut s, &w, PlayInput::default(), 1.0);
        s.combat.orcs.push(Orc::new(Pos2::new(0.0, ORC_REST_Y)));

        let mut charged = false;
        for _ in 0..((4.0 / DT) as usize) {
            s.step(&w, PlayInput::default(), DT);
            if s.combat.orcs[0].state == OrcState::Charging {
                charged = true;
            }
        }
        let orc = &s.combat.orcs[0];
        assert!(charged, "the orc must actually charge for this to test anything");
        assert!(orc.pos.x > 20.0, "and must have travelled, x {}", orc.pos.x);
        // The standoff, not merely a collision: a bare capsule would rest at
        // 200 - ORC_CAPSULE_RADIUS = 168, a gap of only 32 px.
        let gap = 200.0 - orc.pos.x;
        assert!(
            gap >= cfg::ORC_WALL_STOP_DISTANCE_PX - 6.0,
            "the orc should keep its {} px standoff, but the gap was only {gap}",
            cfg::ORC_WALL_STOP_DISTANCE_PX
        );
    }

    #[test]
    fn orcs_fall_under_gravity_and_rest_on_the_floor() {
        let w = floor();
        // Spawned high above the floor, far from the player so its AI stays idle.
        let bitmaps = vec![bitmap("orc_tool", -64.0, -64.0, 128.0, 128.0)];
        let mut s = PlaySession::new(Pos2::new(3000.0, 400.0), &bitmaps);
        run(&mut s, &w, PlayInput::default(), 3.0);
        let orc = &s.combat.orcs[0];
        assert!(orc.grounded, "the orc should have landed");
        assert!((orc.pos.y - ORC_REST_Y).abs() < 3.0, "orc rest y {}", orc.pos.y);
        assert!(orc.vel.y.abs() < 1.0, "and be at rest, vy {}", orc.vel.y);
    }

    #[test]
    fn nothing_is_hit_after_the_window_closes() {
        let (mut s, w) = duel(54.0);
        // Run the whole swing, then place a fresh orc in reach and finish the swing out.
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_HIT_TO + 2.0 * DT);
        s.combat.orcs[0].power = cfg::ORC_MAX_POWER; // undo the in-window hit
        let remaining = cfg::SLASH_FORWARD_DURATION - cfg::SLASH_FORWARD_HIT_TO - 2.0 * DT;
        run(&mut s, &w, PlayInput::default(), remaining.max(0.0));
        assert_eq!(
            s.combat.orcs[0].power,
            cfg::ORC_MAX_POWER,
            "the blade must be inert after its window closes"
        );
    }

    #[test]
    fn an_orc_just_outside_the_hitbox_edge_is_not_hit() {
        // The forward box reaches REACH + HALF_W ahead; an orc whose near edge sits just
        // beyond that must be missed, unlike one just inside.
        let edge = cfg::SLASH_FORWARD_REACH + cfg::SLASH_FORWARD_HALF_W + cfg::ORC_CAPSULE_RADIUS;
        let (mut s, w) = duel(edge + 4.0);
        s.combat.orcs[0].state = OrcState::Attacking; // hold it still
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert_eq!(s.combat.orcs[0].power, cfg::ORC_MAX_POWER, "just outside must miss");

        let (mut s, w) = duel(edge - 6.0);
        s.combat.orcs[0].state = OrcState::Attacking;
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert!(s.combat.orcs[0].power < cfg::ORC_MAX_POWER, "just inside must hit");
    }

    #[test]
    fn a_ground_swing_caps_the_run_speed_without_stopping_the_player() {
        let (mut s, w) = standing(&[]);
        let cap = cfg::PLAYER_RUN_SPEED * cfg::SLASH_GROUND_SPEED_MULT;
        // Reach full speed first, then swing while still holding right.
        run(&mut s, &w, PlayInput { sim: Input { right: true, ..Input::default() }, ..Default::default() }, 0.5);
        assert!(s.player.vel.x > cap, "running at full speed before the swing");
        let swing = PlayInput { attack: true, sim: Input { right: true, ..Input::default() }, ..Default::default() };
        s.step(&w, swing, DT);
        run(&mut s, &w, PlayInput { sim: Input { right: true, ..Input::default() }, ..Default::default() }, cfg::SLASH_FORWARD_DURATION * 0.5);
        assert!(
            (s.player.vel.x - cap).abs() < 5.0,
            "a ground swing should cap the run at {cap}, got {} (a per-step multiply would decay to ~0)",
            s.player.vel.x
        );
    }

    // ── A10, A11, A12: club damage, i-frames, shield ────────────────────────────

    #[test]
    fn the_club_only_bites_inside_its_derived_window() {
        // 10 frames at 12 fps, damaging over frames 3..=5.
        assert!((cfg::ORC_SWING_DURATION - 10.0 / 12.0).abs() < 1e-6);
        assert!((cfg::ORC_CLUB_ACTIVE_FROM - 0.25).abs() < 1e-6);
        assert!((cfg::ORC_CLUB_ACTIVE_TO - 0.5).abs() < 1e-6);

        let mut orc = Orc::new(Pos2::ZERO);
        orc.state = OrcState::Attacking;
        for (t, want) in [(0.0, false), (0.24, false), (0.3, true), (0.49, true), (0.6, false)] {
            orc.swing_elapsed = t;
            assert_eq!(orc.club_active(), want, "club at {t}s");
        }
        orc.state = OrcState::Idle;
        orc.swing_elapsed = 0.3;
        assert!(!orc.club_active(), "an idle orc never has a live club");
    }

    #[test]
    fn consecutive_club_swings_are_separated_by_the_reattack_delay() {
        let (mut s, w) = orc_and_player(cfg::ORC_ATTACK_RANGE_PX - 30.0);
        s.combat.iframes_left = 1.0e6; // measure the swing rhythm, not the damage
        s.step(&w, PlayInput::default(), DT);
        assert_eq!(s.combat.orcs[0].state, OrcState::Attacking);

        // Time from the end of one club window to the start of the next.
        let mut live = Vec::new();
        let mut t = 0.0;
        let mut was_live = false;
        while t < cfg::ORC_SWING_DURATION * 3.0 {
            s.player.pos = Pos2::new(s.combat.orcs[0].pos.x - 40.0, PLAYER_REST_Y);
            s.step(&w, PlayInput::default(), DT);
            let now = s.combat.orcs[0].club_active();
            if now != was_live {
                live.push((t, now));
                was_live = now;
            }
            t += DT;
        }
        // Expect: window opens, closes, opens again. The gap between one swing's end and
        // the next swing's club must include the reattack delay.
        assert!(live.len() >= 3, "expected at least two club windows, saw {live:?}");
        let (close, _) = live[1];
        let (next_open, _) = live[2];
        let gap = next_open - close;
        let quiet_tail = cfg::ORC_SWING_DURATION - cfg::ORC_CLUB_ACTIVE_TO;
        let expected = quiet_tail + cfg::ORC_REATTACK_DELAY_SEC + cfg::ORC_CLUB_ACTIVE_FROM;
        assert!(
            (gap - expected).abs() < 0.05,
            "gap between club windows was {gap}s, expected ~{expected}s \
             (tail {quiet_tail} + reattack {} + wind-in {})",
            cfg::ORC_REATTACK_DELAY_SEC,
            cfg::ORC_CLUB_ACTIVE_FROM
        );
    }

    #[test]
    fn the_freeze_recovered_window_lapses() {
        let mut orc = Orc::new(Pos2::ZERO);
        orc.freeze_left = cfg::ORC_HIT_FREEZE_SEC;
        let w = World::default();
        let player = Simulation::new(Pos2::new(10_000.0, 0.0)); // far away: stays Idle

        // Run out the freeze; the recovered window opens.
        for _ in 0..((cfg::ORC_HIT_FREEZE_SEC / DT).ceil() as usize + 2) {
            step_one_orc(&mut orc, &player, &w, DT);
        }
        assert!(orc.freeze_recovered_left > 0.0, "the window should open when the freeze ends");

        // And closes again after ORC_FREEZE_RECOVERED_SECS rather than lasting forever.
        for _ in 0..((cfg::ORC_FREEZE_RECOVERED_SECS / DT).ceil() as usize + 2) {
            step_one_orc(&mut orc, &player, &w, DT);
        }
        assert_eq!(orc.freeze_recovered_left, 0.0, "the window must lapse");
    }

    #[test]
    fn a_club_hit_costs_health_knocks_back_and_grants_iframes() {
        let (mut s, w) = orc_and_player(cfg::ORC_ATTACK_RANGE_PX - 30.0);
        let start_health = s.combat.health;
        // Step until the club lands so the knockback is read at the moment it is applied.
        let mut steps = 0;
        while s.combat.health == start_health && steps < 400 {
            s.step(&w, PlayInput::default(), DT);
            steps += 1;
        }
        assert_eq!(s.combat.health, start_health - cfg::ORC_CLUB_DAMAGE, "one point of damage");
        assert!(s.combat.is_invulnerable(), "i-frames should be running");
        assert!(s.player.is_control_locked(), "hurt lockout should suppress input");
        assert!(
            (s.player.vel.x.abs() - cfg::ORC_CLUB_KNOCKBACK_X).abs() < 1.0,
            "knockback should be exactly {} px/s horizontally, got {}",
            cfg::ORC_CLUB_KNOCKBACK_X,
            s.player.vel.x
        );
        assert!(
            (s.player.vel.y + cfg::ORC_CLUB_KNOCKBACK_Y).abs() < 1.0,
            "and exactly -{} vertically, got {}",
            cfg::ORC_CLUB_KNOCKBACK_Y,
            s.player.vel.y
        );
        // The pop must survive the next step: a bounce never takes the shorten gravity.
        s.step(&w, PlayInput::default(), DT);
        assert!(s.player.vel.y < -cfg::ORC_CLUB_KNOCKBACK_Y * 0.5, "pop cancelled, vy {}", s.player.vel.y);

        // A second hit inside the i-frame window must be refused, checked across real
        // club windows with the player held inside the club's reach.
        let after = s.combat.health;
        let mut elapsed = 0.0;
        let mut saw_live_club = false;
        while elapsed < cfg::PLAYER_CLUB_IFRAMES * 0.8 {
            s.player.pos = Pos2::new(s.combat.orcs[0].pos.x - 60.0, PLAYER_REST_Y);
            s.step(&w, PlayInput::default(), DT);
            if s.combat.orcs[0].club_active() {
                saw_live_club = true;
            }
            elapsed += DT;
        }
        assert!(saw_live_club, "the orc's club must actually have been live during the window");
        assert_eq!(s.combat.health, after, "no damage during i-frames");
    }

    #[test]
    fn the_shield_refuses_damage_then_locks_out() {
        let (mut s, w) = orc_and_player(cfg::ORC_ATTACK_RANGE_PX - 30.0);
        let shield = PlayInput { shield: true, ..Default::default() };
        s.step(&w, shield, DT);
        assert!(s.combat.shield.is_active());
        let health = s.combat.health;

        // Through a whole club window while the shield is up.
        run(&mut s, &w, PlayInput::default(), cfg::SHIELD_ACTIVE_SECS - 0.1);
        assert_eq!(s.combat.health, health, "the shield should refuse club damage");

        // It expires and cannot be raised again until the cooldown ends.
        run(&mut s, &w, PlayInput::default(), 0.2);
        assert!(!s.combat.shield.is_active());
        assert!(!s.combat.shield.is_ready(), "should be cooling down");
        s.step(&w, shield, DT);
        assert!(!s.combat.shield.is_active(), "cannot re-raise during the cooldown");
    }

    #[test]
    fn the_shield_becomes_ready_again_after_its_cooldown() {
        let w = floor();
        let mut s = PlaySession::new(Pos2::new(0.0, 400.0), &[]);
        run(&mut s, &w, PlayInput::default(), 1.0);
        s.step(&w, PlayInput { shield: true, ..Default::default() }, DT);
        assert!(s.combat.shield.is_active());
        run(&mut s, &w, PlayInput::default(), cfg::SHIELD_ACTIVE_SECS + cfg::SHIELD_COOLDOWN_SECS + 0.1);
        assert!(s.combat.shield.is_ready(), "ready again after active + cooldown");
    }

    // ── A13, A14, A15: coins, pits, death ───────────────────────────────────────

    #[test]
    fn touching_a_coin_collects_it_and_scores() {
        // Coin centred on the player's resting position.
        let bitmaps = vec![bitmap("coin_tool", -32.0, 455.0 - 32.0, 64.0, 64.0)];
        let (s, _) = standing(&bitmaps);
        assert!(s.combat.coins[0].collected, "should have been picked up");
        assert_eq!(s.combat.score, cfg::COIN_PICKUP_POINTS);
        assert_eq!(s.combat.coins_collected(), 1);
    }

    #[test]
    fn a_coin_out_of_reach_is_not_collected() {
        let bitmaps = vec![bitmap("coin_tool", 2000.0, 100.0, 64.0, 64.0)];
        let (s, _) = standing(&bitmaps);
        assert!(!s.combat.coins[0].collected);
        assert_eq!(s.combat.score, 0);
    }

    #[test]
    fn a_death_pit_kills_regardless_of_health() {
        let bitmaps = vec![bitmap("death_trap_tool", -24.0, 455.0 - 24.0, 48.0, 48.0)];
        let w = floor();
        let mut s = PlaySession::new(Pos2::new(0.0, 400.0), &bitmaps);
        assert_eq!(s.combat.health, cfg::INITIAL_HEALTH);
        run(&mut s, &w, PlayInput::default(), 1.0);
        // The run restarts on death, so health is back to full at the spawn point.
        assert_eq!(s.combat.health, cfg::INITIAL_HEALTH);
        assert_eq!(s.player.pos, Pos2::new(0.0, 400.0), "respawned at the spawn point");
    }

    #[test]
    fn death_restores_orcs_coins_and_score() {
        let (mut s, w) = duel(54.0);
        // Bank a coin and wound the orc.
        s.combat.coins.push(Coin {
            rect: Rect::from_center_size(Pos2::new(0.0, PLAYER_REST_Y), Vec2::splat(64.0)),
            collected: false,
        });
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        assert!(s.combat.coins[0].collected, "coin should have been picked up");
        assert!(s.combat.orcs[0].power < cfg::ORC_MAX_POWER, "orc should be wounded");

        // Drop to the last health point and let the orc's club land.
        s.combat.health = 1;
        s.combat.iframes_left = 0.0;
        // Stay 60 px from the orc (well inside its club box) until it connects. The
        // sword knockback pushes the orc out of reach, and an attacking orc holds its
        // ground until the player is beyond ORC_ATTACK_RANGE_LEAVE_PX, so the player is
        // the one who has to close.
        let mut died = false;
        for _ in 0..600 {
            s.player.pos = Pos2::new(s.combat.orcs[0].pos.x - 60.0, PLAYER_REST_Y);
            s.step(&w, PlayInput::default(), DT);
            if s.just_died {
                died = true;
                break;
            }
        }
        assert!(died, "the club should eventually land and kill at 1 health");

        assert_eq!(s.combat.health, cfg::INITIAL_HEALTH, "health restored");
        assert_eq!(s.combat.orcs[0].power, cfg::ORC_MAX_POWER, "orc restored");
        assert!(s.combat.orcs[0].alive);
        assert_eq!(s.combat.orcs[0].pos, s.combat.orcs[0].home, "orc back at its level position");
        assert!(!s.combat.coins[0].collected, "coin restored");
        assert_eq!(s.combat.score, 0, "score reset");
    }

    #[test]
    fn restart_puts_everything_back() {
        let bitmaps = vec![bitmap("orc_tool", 54.0 - 64.0, 455.0 - 64.0, 128.0, 128.0)];
        let (mut s, w) = standing(&bitmaps);
        run(&mut s, &w, attack_input(), cfg::SLASH_FORWARD_DURATION);
        s.combat.score = 999;
        s.restart();
        assert_eq!(s.combat.orcs[0].power, cfg::ORC_MAX_POWER);
        assert_eq!(s.combat.score, 0);
        assert_eq!(s.combat.health, cfg::INITIAL_HEALTH);
        assert!(s.combat.attack.slash.is_none());
    }

    // ── A19: frame-rate independence ────────────────────────────────────────────

    #[test]
    fn combat_outcomes_match_at_60hz_and_144hz() {
        let bitmaps = vec![
            bitmap("orc_tool", 54.0 - 64.0, 455.0 - 64.0, 128.0, 128.0),
            bitmap("coin_tool", -32.0, 455.0 - 32.0, 64.0, 64.0),
        ];
        let w = floor();
        let input = attack_input();
        let mut a = PlaySession::new(Pos2::new(0.0, 400.0), &bitmaps);
        let mut b = PlaySession::new(Pos2::new(0.0, 400.0), &bitmaps);
        for _ in 0..180 {
            a.advance(&w, input, 1.0 / 60.0);
        }
        for _ in 0..432 {
            b.advance(&w, input, 1.0 / 144.0);
        }
        assert_eq!(a.combat.score, b.combat.score, "same score");
        assert_eq!(a.combat.health, b.combat.health, "same health");
        assert_eq!(a.combat.orcs_alive(), b.combat.orcs_alive(), "same orcs left");
        assert_eq!(a.combat.coins_collected(), b.combat.coins_collected());
    }

    #[test]
    fn advance_ignores_invalid_dt_and_bounds_catch_up() {
        let w = floor();
        let mut s = PlaySession::new(Pos2::new(0.0, 400.0), &[]);
        assert_eq!(s.advance(&w, PlayInput::default(), f32::NAN), 0);
        assert_eq!(s.advance(&w, PlayInput::default(), -1.0), 0);
        assert_eq!(s.advance(&w, PlayInput::default(), 10.0), MAX_STEPS_PER_FRAME);
    }

    // ── Presentation helpers ────────────────────────────────────────────────────

    #[test]
    fn orc_state_labels_are_stable() {
        assert_eq!(OrcState::Idle.label(), "idle");
        assert_eq!(OrcState::PreparingCharge.label(), "wind-up");
        assert_eq!(OrcState::Charging.label(), "charging");
        assert_eq!(OrcState::Attacking.label(), "attacking");
    }

    #[test]
    fn orc_power_fraction_spans_zero_to_one() {
        let mut orc = Orc::new(Pos2::ZERO);
        assert_eq!(orc.power_fraction(), 1.0);
        orc.power = cfg::ORC_MAX_POWER * 0.5;
        assert_eq!(orc.power_fraction(), 0.5);
        orc.power = -10.0;
        assert_eq!(orc.power_fraction(), 0.0);
    }
}
