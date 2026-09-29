//! Asking a local model for a Hollow-Knight-style area, and refusing to deliver one that
//! cannot be played.
//!
//! The model authors the level JSON itself, coordinates and all. This module only asks,
//! checks and — when the model's own geometry will not do — repairs. The order is always
//! the same: ask, validate what came back, prove a route through it with
//! [`crate::traversal`], and on failure tell the model exactly what went wrong and ask
//! again. Only when that is exhausted does [`crate::repair`] touch the geometry, and when
//! it does, the result says so.
//!
//! Requests are staged — the area's outline first, then one request per chamber — because
//! a whole area is more output than CPU-only inference returns reliably in one reply. The
//! staging bounds each reply's length; it takes no authorship away from the model.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egui::{Pos2, Vec2};

use crate::ai_client::{parse_reply, ModelClient, ModelError, Request};
use crate::game_config as cfg;
use crate::level_data::{LevelData, LevelEntity};
use crate::traversal::{self, Problem, Verdict};

/// Every entity name the model is allowed to use: the editor's toolbox, and nothing else.
pub const POLYGON_TYPES: [&str; 3] = ["wall_tool", "blocker_tool", "polygon_tool"];
pub const BITMAP_NAMES: [&str; 4] = ["coin_tool", "death_trap_tool", "orc_tool", "rope_tool"];

/// How many times one chamber may be asked for again before the attempt is abandoned.
pub const MAX_CHAMBER_RETRIES: usize = 2;
/// A rope is always this thick, per `LEVEL_FORMAT.md`; only its length varies.
pub const ROPE_THICKNESS: f32 = 6.0;
/// How many times the whole area may be re-asked after failing the route proof. Each round
/// costs a full generation, so this is deliberately small; D8 fixes it at two.
pub const MAX_PROOF_RETRIES: usize = 2;

/// What the user asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationParams {
    /// The user's own words: the area, its mood, its difficulty.
    pub prompt: String,
    /// The extent to generate into.
    pub extent: Vec2,
    /// How many chambers to ask for.
    pub chambers: usize,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self { prompt: String::new(), extent: Vec2::new(6000.0, 3000.0), chambers: 5 }
    }
}

/// Where a run has got to, for the progress panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Contacting,
    Outline,
    Chamber { index: usize, total: usize, name: String },
    Proving,
    /// The model is being asked to fix its own geometry.
    Retrying { attempt: usize, why: String },
    Repairing,
}

/// Why a generation did not produce a level.
#[derive(Clone, Debug, PartialEq)]
pub enum GenError {
    Model(ModelError),
    /// The model's replies could not be made into a valid area after its retries.
    Invalid(String),
    /// A valid area, but no route through it, and repair could not make one.
    Unprovable(String),
    Cancelled,
    /// The worker stopped without producing a result, and was not asked to.
    Worker,
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::Model(e) => write!(f, "{e}"),
            GenError::Invalid(why) => write!(
                f,
                "The model could not produce a usable area: {why} \
                 The level you had open has not been touched."
            ),
            GenError::Unprovable(why) => write!(
                f,
                "No playable route could be produced. {why} \
                 The level you had open has not been touched."
            ),
            GenError::Cancelled => write!(f, "Generation was cancelled."),
            GenError::Worker => write!(
                f,
                "Generation stopped unexpectedly before it finished. The level you had open \
                 has not been touched."
            ),
        }
    }
}

impl From<ModelError> for GenError {
    fn from(e: ModelError) -> Self {
        GenError::Model(e)
    }
}

/// A finished, proven level and how it came about.
#[derive(Clone, Debug)]
pub struct Generated {
    pub level: LevelData,
    /// Empty when the delivered geometry is the model's own; otherwise what repair changed.
    pub repairs: Vec<String>,
    /// Anything the model described too incompletely to place, and was left out. Not an
    /// error — the level was delivered — but the user is told rather than left to wonder.
    pub skipped: Vec<String>,
    /// The moves the proven route uses, for the report.
    pub route_moves: usize,
    /// How long the proven route takes to run, in simulated seconds.
    pub route_secs: f32,
    pub chambers: Vec<String>,
}

impl Generated {
    /// Whether the delivered level is exactly what the model wrote.
    pub fn is_the_models_own(&self) -> bool {
        self.repairs.is_empty()
    }
}

/// One chamber of the area, as the model described it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChamberPlan {
    pub name: String,
    pub rect: [f32; 4],
    pub role: String,
}

/// Where two chambers meet. Both must put solid floor at this point, or the route dies at
/// the seam between them.
#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub from: usize,
    pub to: usize,
    pub at: Pos2,
}

/// The area outline the model returns first.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub chambers: Vec<ChamberPlan>,
    pub connections: Vec<Connection>,
    pub spawn: Pos2,
    pub exit: Pos2,
}

impl Outline {
    /// The doorways chamber `index` takes part in: where it meets a neighbour, and which
    /// neighbour that is. The one definition of what a chamber must put floor at, so the
    /// prompt and anything checking it cannot drift apart.
    pub fn doorways(&self, index: usize) -> Vec<(Pos2, usize)> {
        self.connections
            .iter()
            .filter_map(|c| match (c.from, c.to) {
                (from, to) if from == index => Some((c.at, to)),
                (from, to) if to == index => Some((c.at, from)),
                _ => None,
            })
            .collect()
    }
}

/// The rules every request carries: what the level format is, what may be placed in it,
/// and — most importantly — what the character can actually do, so the model has the reach
/// numbers in front of it while it is choosing coordinates.
pub fn system_prompt(extent: Vec2) -> String {
    format!(
        "You design levels for a 2D Metroidvania in the style of Hollow Knight, and you \
         answer only with JSON.\n\n\
         COORDINATES: world pixels, origin at the top left, x to the right, y DOWNWARD. \
         The level is {w:.0} wide and {h:.0} tall. Every coordinate must lie inside that.\n\n\
         WHAT YOU MAY PLACE, and nothing else:\n\
         - {{\"type\":\"polygon\",\"vertices\":[[x,y],...],\"polygon_type\":\"wall_tool\"}} \
         — solid ground the character can also CLIMB. Use it for climbable faces.\n\
         - {{\"type\":\"polygon\",\"vertices\":[[x,y],...],\"polygon_type\":\"blocker_tool\"}} \
         — solid, not climbable.\n\
         - {{\"type\":\"polygon\",\"vertices\":[[x,y],...],\"polygon_type\":\"polygon_tool\"}} \
         — solid, not climbable.\n\
         - {{\"type\":\"bitmap\",\"position\":[x,y],\"bitmap_name\":\"coin_tool\",\"size\":[64,64]}} \
         — a reward to collect.\n\
         - {{\"type\":\"bitmap\",\"position\":[x,y],\"bitmap_name\":\"orc_tool\",\"size\":[128,128]}} \
         — an enemy.\n\
         - {{\"type\":\"bitmap\",\"position\":[x,y],\"bitmap_name\":\"death_trap_tool\",\"size\":[48,48]}} \
         — a hazard that kills on contact.\n\
         - {{\"type\":\"bitmap\",\"position\":[x,y],\"bitmap_name\":\"rope_tool\",\"size\":[6,LENGTH]}} \
         — a rope hanging DOWN from position; LENGTH is at least 60.\n\
         Polygons need at least 3 vertices. Every polygon is solid: the character cannot \
         pass through it.\n\n\
         WHAT THE CHARACTER CAN DO — respect these or the level is impossible:\n\
         - Runs at {run:.0} px/s.\n\
         - Jumps {jump:.0} px high. There is NO double jump.\n\
         - Dashes {dash:.0} px horizontally, once in the air per jump.\n\
         - Wall jumps {wj_h:.0} px up and {wj_x:.0} px across, off any solid face.\n\
         - Climbs a wall_tool face at {climb:.0} px/s.\n\
         So: a horizontal gap wider than about {dash:.0} px is impassable. A rise taller \
         than {jump:.0} px needs a wall to jump off or a wall_tool face to climb. Leave \
         ledges at least 200 px wide to land on.\n\n\
         Answer with JSON only. No explanation, no markdown fence.",
        w = extent.x,
        h = extent.y,
        run = cfg::PLAYER_RUN_SPEED,
        jump = cfg::JUMP_HEIGHT,
        dash = cfg::DASH_DISTANCE,
        wj_h = cfg::WALL_JUMP_HEIGHT,
        wj_x = cfg::WALL_JUMP_HORIZONTAL_DISTANCE,
        climb = cfg::CLIMB_SPEED,
    )
}

/// The request that asks for the shape of the area.
pub fn outline_prompt(params: &GenerationParams) -> String {
    format!(
        "Design the outline of an area: {n} chambers joined into one connected place, with \
         a route from the start of the area to its end, and optional side branches.\n\n\
         The user asks for: {prompt}\n\n\
         Answer with exactly this JSON:\n\
         {{\"chambers\":[{{\"name\":\"...\",\"rect\":[x,y,width,height],\"role\":\"...\"}}],\
         \"connections\":[{{\"from\":0,\"to\":1,\"at\":[x,y]}}],\
         \"spawn\":[x,y],\"exit\":[x,y]}}\n\n\
         The chamber rectangles must tile the level without overlapping, and touching \
         chambers must share an edge so the character can pass between them. \"spawn\" is \
         where the run begins, inside the first chamber and above its floor. \"exit\" is \
         where the route ends, inside the last chamber. \"role\" is a few words about what \
         the chamber is for.\n\n\
         \"connections\" is the important part. Chambers are filled in one at a time, so \
         they only join up if you say exactly where they join. For every pair of chambers \
         the character must be able to walk between, give a connection: the indices of \
         the two chambers (0-based, in the order you listed them) and the point where they \
         meet. Put that point ON their shared edge, at the height the floor will be on both \
         sides. Two rules about the set of connections, both of which will be checked:\n\
         - EVERY chamber must be reachable from the first one by following connections. A \
         chamber nothing connects to is a room the player can never enter.\n\
         - You may give more than one connection between the same pair of chambers when \
         there really are two passages, an upper and a lower say, but each must be at its \
         own point. Do not state the same doorway twice.",
        n = params.chambers,
        prompt = if params.prompt.trim().is_empty() { "a dark, ruined underground area" } else { params.prompt.trim() },
    )
}

/// The request that asks for one chamber's contents.
pub fn chamber_prompt(params: &GenerationParams, outline: &Outline, index: usize, note: Option<&str>) -> String {
    let chamber = &outline.chambers[index];
    let [x, y, w, h] = chamber.rect;
    let mut text = format!(
        "Fill the chamber \"{name}\" ({role}) with entities.\n\n\
         It occupies x {x:.0} to {x1:.0}, y {y:.0} to {y1:.0}. Everything you place must be \
         inside that rectangle.\n\n\
         The area as a whole: {prompt}\n\n\
         Give it a floor, ledges to climb between, and the hazards, enemies and rewards \
         that suit its role. Remember the character's reach: nothing it must cross may be \
         wider than {dash:.0} px or taller than {jump:.0} px without a wall to work with.\n\n\
         Answer with exactly this JSON: {{\"entities\":[ ... ]}}",
        name = chamber.name,
        role = chamber.role,
        x1 = x + w,
        y1 = y + h,
        prompt = if params.prompt.trim().is_empty() { "a dark, ruined underground area" } else { params.prompt.trim() },
        dash = cfg::DASH_DISTANCE,
        jump = cfg::JUMP_HEIGHT,
    );
    if index == 0 {
        text.push_str(&format!(
            "\n\nThe run begins at ({sx:.0}, {sy:.0}): put solid floor beneath that point.",
            sx = outline.spawn.x,
            sy = outline.spawn.y
        ));
    }
    if index + 1 == outline.chambers.len() {
        text.push_str(&format!(
            "\n\nThe route ends at ({ex:.0}, {ey:.0}): put solid floor beneath that point too.",
            ex = outline.exit.x,
            ey = outline.exit.y
        ));
    }
    // The connections this chamber takes part in. Without these the chamber is designed in
    // isolation, its floor ends up at whatever height suits it, and the step where it meets
    // its neighbour is one the character cannot climb — so the route dies at the seam.
    let joins = outline.doorways(index);
    if !joins.is_empty() {
        text.push_str(
            "\n\nThis chamber joins its neighbours at the points below. Each one is a \
             doorway the character must be able to walk through, so put continuous solid \
             floor at each point, level with it, reaching to the chamber's edge there, and \
             make sure a route runs along your floor from one of these points to the others:",
        );
        for (at, other) in joins {
            text.push_str(&format!(
                "\n- ({x:.0}, {y:.0}), joining \"{name}\"",
                x = at.x,
                y = at.y,
                name = outline.chambers[other].name,
            ));
        }
    }
    if let Some(note) = note {
        text.push_str(&format!("\n\nYour previous attempt was rejected: {note}\nFix it."));
    }
    text
}

/// A number a model wrote, however it wrote it.
///
/// Models quote numbers surprisingly often, so `"1200"` is accepted as well as `1200`.
fn read_number(value: &serde_json::Value) -> Option<f32> {
    let n = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    } as f32;
    n.is_finite().then_some(n)
}

/// A short rendering of whatever the model actually wrote, for an error message. Long values
/// are cut off: the point is to show the shape, and the whole of a big object would bury it.
fn shown(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 80 {
        text
    } else {
        format!("{}…", text.chars().take(80).collect::<String>())
    }
}

/// A pair of numbers a model wrote, in any unambiguous spelling.
///
/// `[x, y]` is what the prompt asks for, but a model that has been told the shape will still
/// sometimes write `{"x": .., "y": ..}` — and will keep writing it when re-asked, because to
/// it the two are the same thing. Refusing an equivalent spelling burns an inference round to
/// no purpose, so any of these is accepted and everything downstream — the vocabulary check,
/// the bounds check, the route proof — is unchanged:
///
/// - `[x, y]`, and a longer array, whose first two entries are taken
/// - `{"x": x, "y": y}`, and `{"width": w, "height": h}` for a size
/// - numbers written as strings
///
/// `alternate_keys` are the object keys to accept besides `x` and `y`, so a size can also
/// arrive as width and height.
fn read_pair(
    value: &serde_json::Value,
    alternate_keys: (&str, &str),
) -> Option<[f32; 2]> {
    match value {
        // A pair wrapped in another array — `[[150, 2672]]` — which a model writes when it is
        // thinking of a list of points and has only one. Unwrap it rather than refuse it.
        serde_json::Value::Array(items) if items.len() == 1 => {
            read_pair(&items[0], alternate_keys)
        }
        serde_json::Value::Array(items) if items.len() >= 2 => {
            // Two numbers is the ordinary case. Two *pairs* is not a point at all, so it is
            // left to fail rather than silently taking the first.
            Some([read_number(&items[0])?, read_number(&items[1])?])
        }
        serde_json::Value::Object(_) => {
            let get = |a: &str, b: &str| value.get(a).or_else(|| value.get(b));
            let first = get("x", alternate_keys.0)?;
            let second = get("y", alternate_keys.1)?;
            Some([read_number(first)?, read_number(second)?])
        }
        _ => None,
    }
}

/// A point from `key`, or from sibling fields when the model put them there instead.
fn read_point(
    parent: &serde_json::Value,
    key: &str,
    alternate_keys: (&str, &str),
    what: &str,
) -> Result<[f32; 2], String> {
    if let Some(value) = parent.get(key) {
        return read_pair(value, alternate_keys).ok_or_else(|| {
            format!(
                "{what}'s \"{key}\" is {}, which is not a pair of numbers; write it as \
                 [x, y]",
                shown(value)
            )
        });
    }
    // No `key` at all: some replies put the numbers directly on the entity.
    if let Some(pair) = read_pair(parent, alternate_keys) {
        return Ok(pair);
    }
    Err(format!(
        "{what} has no \"{key}\"; write it as \"{key}\": [x, y]"
    ))
}

/// The keys a model might put an entity's name or type under, besides the one asked for.
/// `bitmap_name` and `polygon_type` are what the prompt shows; the rest are what a model
/// reaches for when it is paraphrasing rather than copying.
const NAME_KEYS: [&str; 7] =
    ["bitmap_name", "name", "sprite", "asset", "bitmap", "tool", "kind"];
const POLYGON_TYPE_KEYS: [&str; 5] = ["polygon_type", "kind", "tool", "style", "name"];

/// The size to use for a sprite whose size the model left out. These match `LEVEL_FORMAT.md`,
/// and for everything but a rope they barely matter: `Entity::from_level_entity` re-derives a
/// non-rope bitmap's size from the toolbox when the level is loaded. A missing size is
/// therefore not worth failing a generation over.
fn default_size(name: &str) -> [f32; 2] {
    match name {
        "orc_tool" => [128.0, 128.0],
        "death_trap_tool" => [48.0, 48.0],
        cfg::ROPE_BITMAP => [ROPE_THICKNESS, 300.0],
        _ => [64.0, 64.0],
    }
}

/// The keys an object actually has, for an error message that is worth reading.
fn keys_of(value: &serde_json::Value) -> String {
    match value.as_object() {
        Some(map) if !map.is_empty() => map
            .keys()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "nothing".to_string(),
    }
}

/// The first value under any of `keys` that is a string.
///
/// First *present*, which is rarely what a caller wants on its own: an entity can carry a
/// human label under one key and the real answer under another. Use [`read_known`] to look
/// for a value that means something, and keep this for saying what was found when none does.
fn read_named(item: &serde_json::Value, keys: &[&'static str]) -> Option<(&'static str, String)> {
    keys.iter().find_map(|k| {
        item.get(*k)
            .and_then(|v| v.as_str())
            .map(|v| (*k, v.trim().to_string()))
    })
}

/// The first value under any of `keys` that is one of `vocabulary`.
///
/// A model writing `{"name": "spiked pit", "sprite": "death_trap_tool"}` has given both a
/// label and an answer, and taking whichever came first in the key list read the label and
/// missed the answer. That mattered in both directions: the entity was condemned as
/// unnameable when it had named itself perfectly well, and — worse — if it also carried
/// vertices it was taken for geometry and a hazard became a platform, which the route proof
/// can never catch because added floor only ever makes a route easier.
fn read_known(
    item: &serde_json::Value,
    keys: &[&'static str],
    vocabulary: &[&str],
) -> Option<(&'static str, String)> {
    keys.iter().find_map(|k| {
        item.get(*k)
            .and_then(|v| v.as_str())
            .map(|v| v.trim())
            .filter(|v| vocabulary.contains(v))
            .map(|v| (*k, v.to_string()))
    })
}

/// Read the outline out of a model reply.
pub fn parse_outline(reply: &str, extent: Vec2) -> Result<Outline, String> {
    let value = parse_reply(reply).map_err(|e| e.to_string())?;
    let chambers_json = value
        .get("chambers")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "the outline has no \"chambers\" array".to_string())?;
    if chambers_json.is_empty() {
        return Err("the outline has no chambers in it".into());
    }
    let mut chambers = Vec::new();
    for (i, c) in chambers_json.iter().enumerate() {
        let rect_value = c
            .get("rect")
            .ok_or_else(|| format!("chamber {i} has no \"rect\""))?;
        // `[x, y, width, height]` is what is asked for; `{x, y, width, height}` is the other
        // spelling a model reaches for, and read_number handles a quoted number either way.
        let nums: [f32; 4] = match rect_value {
            serde_json::Value::Array(items) if items.len() >= 4 => {
                let mut out = [0.0_f32; 4];
                for (j, n) in items.iter().take(4).enumerate() {
                    out[j] = read_number(n).ok_or_else(|| {
                        format!(
                            "chamber {i}'s rect holds {}, which is not a usable number",
                            shown(n)
                        )
                    })?;
                }
                out
            }
            serde_json::Value::Object(_) => {
                let get = |key: &str| -> Result<f32, String> {
                    rect_value
                        .get(key)
                        .and_then(read_number)
                        .ok_or_else(|| format!("chamber {i}'s rect has no usable \"{key}\""))
                };
                [get("x")?, get("y")?, get("width")?, get("height")?]
            }
            other => {
                return Err(format!(
                    "chamber {i}'s rect is {}, which is not [x, y, width, height]",
                    shown(other)
                ))
            }
        };
        if nums[2] <= 0.0 || nums[3] <= 0.0 {
            return Err(format!("chamber {i} has no area"));
        }
        if nums[0] < 0.0
            || nums[1] < 0.0
            || nums[0] + nums[2] > extent.x + 1.0
            || nums[1] + nums[3] > extent.y + 1.0
        {
            return Err(format!(
                "chamber {i} spans x {:.0}..{:.0}, y {:.0}..{:.0}, which does not fit in the \
                 {:.0} by {:.0} level",
                nums[0],
                nums[0] + nums[2],
                nums[1],
                nums[1] + nums[3],
                extent.x,
                extent.y
            ));
        }
        chambers.push(ChamberPlan {
            name: c.get("name").and_then(|n| n.as_str()).unwrap_or("chamber").to_string(),
            rect: nums,
            role: c.get("role").and_then(|n| n.as_str()).unwrap_or("").to_string(),
        });
    }
    let point = |key: &str| -> Result<Pos2, String> {
        let [x, y] = read_point(&value, key, ("x", "y"), "the outline")?;
        if x < 0.0 || y < 0.0 || x > extent.x || y > extent.y {
            return Err(format!("\"{key}\" at ({x:.0}, {y:.0}) is outside the level"));
        }
        Ok(Pos2::new(x, y))
    };
    // Connections. A single-chamber area legitimately has none; anything larger must say
    // where its chambers meet, because that is what stops the route dying at a seam.
    let mut connections = Vec::new();
    if let Some(list) = value.get("connections").and_then(|c| c.as_array()) {
        for (i, c) in list.iter().enumerate() {
            let index = |key: &str| -> Result<usize, String> {
                let n = c
                    .get(key)
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| format!("connection {i} has no numeric \"{key}\""))?
                    as usize;
                if n >= chambers.len() {
                    return Err(format!(
                        "connection {i} names chamber {n}, but there are only {} chambers",
                        chambers.len()
                    ));
                }
                Ok(n)
            };
            let from = index("from")?;
            let to = index("to")?;
            if from == to {
                return Err(format!("connection {i} joins chamber {from} to itself"));
            }
            let [x, y] = read_point(c, "at", ("x", "y"), &format!("connection {i}"))?;
            if x < 0.0 || y < 0.0 || x > extent.x || y > extent.y {
                return Err(format!(
                    "connection {i} meets at ({x:.0}, {y:.0}), which is outside the level"
                ));
            }
            // Two passages between the same pair of chambers — an upper route and a lower
            // one — is ordinary design for this kind of level, so several doorways between a
            // pair are allowed. Only the same doorway stated twice is refused, since that is
            // a contradiction rather than a second way through.
            let at = Pos2::new(x, y);
            if connections.iter().any(|c: &Connection| {
                let same_pair = (c.from, c.to) == (from, to) || (c.to, c.from) == (from, to);
                same_pair && (c.at - at).length() < 1.0
            }) {
                return Err(format!(
                    "connection {i} repeats a doorway between chambers {from} and {to} at the \
                     same point; give each passage its own point, or give only one"
                ));
            }
            connections.push(Connection { from, to, at });
        }
    }
    if chambers.len() > 1 && connections.is_empty() {
        return Err(format!(
            "the outline has {} chambers and no connections; every pair of adjoining \
             chambers must say where they meet",
            chambers.len()
        ));
    }

    // There has to be a way through: a chain of doorways from the first chamber to the last.
    // Chambers that join nothing are a level with a room you cannot reach, and the route
    // proof would reject it after a full round of generation — better to catch it here.
    if chambers.len() > 1 {
        let mut reached = vec![false; chambers.len()];
        reached[0] = true;
        let mut queue = vec![0usize];
        while let Some(n) = queue.pop() {
            for c in &connections {
                let other = match (c.from, c.to) {
                    (from, to) if from == n => Some(to),
                    (from, to) if to == n => Some(from),
                    _ => None,
                };
                if let Some(other) = other {
                    if !reached[other] {
                        reached[other] = true;
                        queue.push(other);
                    }
                }
            }
        }
        if let Some(stranded) = reached.iter().position(|r| !r) {
            return Err(format!(
                "no chain of connections reaches chamber {stranded} (\"{}\") from the first \
                 one, so there is no route through the area",
                chambers[stranded].name
            ));
        }
    }

    Ok(Outline { chambers, connections, spawn: point("spawn")?, exit: point("exit")? })
}

/// Read one chamber's entities out of a model reply, rejecting anything the editor cannot
/// place. This is where an invented entity name stops: nothing that fails here is ever
/// turned into an entity, so no unknown name can reach the canvas.
///
/// Returns the entities and a note for anything left out. An entity the model described
/// incorrectly is an error and the chamber is asked for again; an entity it simply failed to
/// identify is dropped with a note, because one unnameable decoration is not worth discarding
/// a whole chamber over. If that leaves nothing usable, it is an error after all.
pub fn parse_entities(reply: &str, extent: Vec2) -> Result<(Vec<LevelEntity>, Vec<String>), String> {
    let value = parse_reply(reply).map_err(|e| e.to_string())?;
    let array = value
        .get("entities")
        .and_then(|e| e.as_array())
        .or_else(|| value.as_array())
        .ok_or_else(|| "the reply has no \"entities\" array".to_string())?;

    let mut out = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (i, item) in array.iter().enumerate() {
        // What kind of entity this is. `"type": "bitmap"` or `"polygon"` is what is asked
        // for, but a model will also write the tool name straight into `type`, or leave the
        // field out and let the contents speak. An unrecognised `type` is still an error:
        // that is a mistake worth reporting, where an absent one is worth inferring.
        let declared = item.get("type").and_then(|t| t.as_str()).map(|t| t.trim());
        //
        // Vertices decide it — but only when nothing claims to be a sprite. An entity that
        // says "bitmap" and carries vertices, naming no sprite, is collision geometry with
        // the wrong label, and treating it as a nameless sprite would drop a floor as though
        // it were a decoration. An entity that *does* name a sprite is taken at its word,
        // because silently turning a death trap into a platform is the one mistake the route
        // proof can never catch: added floor makes a route easier, never harder.
        //
        // The vertices must also be a non-empty array. A model writing one entity shape with
        // nulls in the unused fields — `"vertices": null` on a coin — is common, and reading
        // that as a polygon would condemn a whole chamber.
        let has_vertices = item
            .get("vertices")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());
        // What counts as claiming to be a sprite: saying so in `type`, using the key the
        // prompt actually asks for (so an invented name there is still held to the
        // vocabulary), or putting a real sprite name under one of the other name keys. Not
        // merely having a string under a key that both a sprite and a polygon might use —
        // `"tool": "wall_tool"` is a polygon saying what kind of polygon it is.
        let claims_a_sprite = declared.is_some_and(|t| BITMAP_NAMES.contains(&t))
            || item.get("bitmap_name").and_then(|v| v.as_str()).is_some()
            || read_known(item, &NAME_KEYS, &BITMAP_NAMES).is_some();
        let kind: &str = if has_vertices && !claims_a_sprite {
            "polygon"
        } else {
            match declared {
            Some("bitmap") => "bitmap",
            Some("polygon") => "polygon",
            Some(t) if POLYGON_TYPES.contains(&t) => "polygon",
            Some(t) if BITMAP_NAMES.contains(&t) => "bitmap",
            Some(other) => {
                return Err(format!(
                    "entity {i} has type {other:?}; only \"polygon\" and \"bitmap\" exist"
                ))
            }
            None if read_named(item, &NAME_KEYS).is_some() || item.get("position").is_some() => {
                "bitmap"
            }
            None => {
                return Err(format!(
                    "entity {i} has no \"type\" and nothing to tell one from its contents; it \
                     has {}",
                    keys_of(item)
                ))
            }
            }
        };
        match kind {
            "polygon" => {
                let verts = item
                    .get("vertices")
                    .and_then(|v| v.as_array())
                    .map(|v| {
                        // The same wrapping one level up: a list of vertices inside a list.
                        match v.as_slice() {
                            [serde_json::Value::Array(inner)]
                                if inner.len() >= 3
                                    && inner.iter().all(|p| read_pair(p, ("x", "y")).is_some()) =>
                            {
                                inner
                            }
                            _ => v,
                        }
                    })
                    .ok_or_else(|| format!("polygon {i} has no \"vertices\""))?;
                if verts.len() < 3 {
                    return Err(format!(
                        "polygon {i} has {} vertices; a polygon needs at least 3",
                        verts.len()
                    ));
                }
                let mut points = Vec::with_capacity(verts.len());
                for v in verts {
                    let [x, y] = read_pair(v, ("x", "y")).ok_or_else(|| {
                        format!(
                            "polygon {i} has a vertex {}, which is not a pair of numbers; write \
                             each vertex as [x, y]",
                            shown(v)
                        )
                    })?;
                    if x < 0.0 || y < 0.0 || x > extent.x || y > extent.y {
                        return Err(format!(
                            "polygon {i} has a vertex at ({x:.0}, {y:.0}), outside the level \
                             ({w:.0} by {h:.0})",
                            w = extent.x,
                            h = extent.y
                        ));
                    }
                    points.push([x, y]);
                }
                // The declared `type`, when it was the tool name itself, settles it.
                let polygon_type = match declared {
                    Some(t) if POLYGON_TYPES.contains(&t) => t.to_string(),
                    _ => {
                        // Otherwise the first of these keys holding a real polygon type. A
                        // key holding something else is a label, not a mistake — `"name":
                        // "north floor"` is a chamber's own note — so it is passed over
                        // rather than rejected, and an explicit `polygon_type` that is not a
                        // real one is still reported.
                        match read_known(item, &POLYGON_TYPE_KEYS, &POLYGON_TYPES) {
                            Some((_, v)) => v,
                            _ => match item.get("polygon_type").and_then(|t| t.as_str()) {
                                Some(bad) => {
                                    return Err(format!(
                                        "polygon {i} has polygon_type {bad:?}, which is not one \
                                         of {:?}",
                                        POLYGON_TYPES
                                    ))
                                }
                                None => "polygon_tool".to_string(),
                            },
                        }
                    }
                };
                out.push(LevelEntity::Polygon {
                    vertices: points,
                    polygon_type: Some(polygon_type),
                    color: None,
                    // Geometry only. Dressing a blocker in a pattern is something the
                    // user does afterwards; the model is not asked for one.
                    pattern: None,
                });
            }
            "bitmap" => {
                // The declared `type`, when it was the sprite name itself, settles it.
                let name = match declared {
                    Some(t) if BITMAP_NAMES.contains(&t) => t.to_string(),
                    // A key holding a real sprite name wins, wherever it is and whatever
                    // else the entity says. Only when no key holds one does a name that is
                    // not a sprite become an error.
                    _ => match read_known(item, &NAME_KEYS, &BITMAP_NAMES)
                        .or_else(|| read_named(item, &NAME_KEYS))
                    {
                        Some((_, v)) if BITMAP_NAMES.contains(&v.as_str()) => v,
                        // Something was named, but it is not a sprite this editor has. Report
                        // it, which is what catches an invented name.
                        Some((key, v)) => {
                            return Err(format!(
                                "bitmap {i}'s \"{key}\" is {v:?}, which is not one of {:?}",
                                BITMAP_NAMES
                            ))
                        }
                        None => {
                            // Nothing here says what sprite this is, and nothing can guess.
                            // A bitmap is a coin, an enemy, a hazard or a rope — never
                            // collision — so dropping one costs the level a decoration, and
                            // whether it mattered is a question the route proof answers a
                            // moment later. Failing the whole chamber over it would throw
                            // away every other entity in it, and the ten minutes that made
                            // them. It is reported, never silent.
                            skipped.push(format!(
                                "entity {i} said only {} and never which sprite it was, so it \
                                 was left out",
                                keys_of(item)
                            ));
                            continue;
                        }
                    },
                };
                let position = read_point(item, "position", ("x", "y"), &format!("bitmap {i}"))?;
                // A size may also arrive as width and height, which is the other spelling a
                // model reaches for. Left out altogether, the sprite's usual size is used.
                let size = match item.get("size").or_else(|| item.get("width")) {
                    Some(_) => read_point(item, "size", ("width", "height"), &format!("bitmap {i}"))?,
                    None => default_size(&name),
                };
                if size[0] <= 0.0 || size[1] <= 0.0 {
                    return Err(format!(
                        "bitmap {i} has size [{:.0}, {:.0}]; both must be greater than zero",
                        size[0], size[1]
                    ));
                }
                if position[0] < 0.0
                    || position[1] < 0.0
                    || position[0] > extent.x
                    || position[1] > extent.y
                {
                    return Err(format!(
                        "bitmap {i} sits at ({:.0}, {:.0}), outside the level",
                        position[0], position[1]
                    ));
                }
                if name == cfg::ROPE_BITMAP {
                    if (size[0] - ROPE_THICKNESS).abs() > 0.01 {
                        return Err(format!(
                            "rope {i} has thickness {:.0}; a rope's thickness is always {ROPE_THICKNESS:.0}",
                            size[0]
                        ));
                    }
                    if size[1] < 60.0 {
                        return Err(format!(
                            "rope {i} is {:.0} px long; a rope must be at least 60 px",
                            size[1]
                        ));
                    }
                }
                out.push(LevelEntity::Bitmap { position, bitmap_name: name, size });
            }
            other => {
                return Err(format!(
                    "entity {i} has type {other:?}; only \"polygon\" and \"bitmap\" exist"
                ))
            }
        }
    }
    // If a reply was mostly unusable, asking again is better than building from the scraps.
    if !array.is_empty() && out.is_empty() {
        return Err(format!(
            "none of the {} entities could be used: {}",
            array.len(),
            skipped.join("; ")
        ));
    }
    Ok((out, skipped))
}

/// Which chamber a point falls in, if any.
pub fn chamber_containing(outline: &Outline, point: Pos2) -> Option<usize> {
    outline.chambers.iter().position(|c| {
        let [x, y, w, h] = c.rect;
        point.x >= x && point.x <= x + w && point.y >= y && point.y <= y + h
    })
}

/// The collision world and the route question a level poses.
pub fn problem_for(level: &LevelData, spawn: Pos2, exit: Pos2, extent: Vec2) -> Problem {
    Problem {
        world: crate::repair::world_of(level),
        spawn,
        exit,
        level_size: Some(extent),
    }
}

/// Generate an area, prove it, and return it — or say why not.
///
/// `progress` is called as each stage begins; `cancel` is checked between every request and
/// every proof, so a cancelled run stops without delivering anything.
pub fn generate(
    client: &mut dyn ModelClient,
    params: &GenerationParams,
    progress: &mut dyn FnMut(Progress),
    cancel: &Arc<AtomicBool>,
) -> Result<Generated, GenError> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    let system = system_prompt(params.extent);

    let ask = |client: &mut dyn ModelClient, user: String| -> Result<String, GenError> {
        if cancelled() {
            return Err(GenError::Cancelled);
        }
        client
            .complete(&Request { system: system.clone(), user, json_only: true })
            .map_err(GenError::from)
    };

    progress(Progress::Contacting);

    // Stage one: the shape of the area.
    progress(Progress::Outline);
    let mut outline = None;
    let mut last_problem = String::new();
    for attempt in 0..=MAX_CHAMBER_RETRIES {
        let note = (attempt > 0).then(|| format!("{last_problem}"));
        let mut user = outline_prompt(params);
        if let Some(note) = note {
            user.push_str(&format!("\n\nYour previous attempt was rejected: {note}\nFix it."));
        }
        let reply = ask(client, user)?;
        match parse_outline(&reply, params.extent) {
            Ok(o) => {
                outline = Some(o);
                break;
            }
            Err(why) => last_problem = why,
        }
    }
    let outline = outline.ok_or_else(|| {
        GenError::Invalid(format!("its outline never came back usable — {last_problem}."))
    })?;

    let mut proof_note: Option<String> = None;
    // Each chamber's accepted entities, kept between proof attempts: a chamber that is
    // fine is not asked for again. On this hardware every chamber costs minutes, so
    // re-asking the whole area to fix one gap would be the difference between one wait and
    // several.
    let mut chamber_entities: Vec<Option<Vec<LevelEntity>>> = vec![None; outline.chambers.len()];
    // One list per chamber rather than one for the area: a chamber that is re-asked
    // replaces its notes, so the report cannot name a drop from a reply that was thrown
    // away, or name the same drop twice.
    let mut skipped_by_chamber: Vec<Vec<String>> = vec![Vec::new(); outline.chambers.len()];
    // Which chambers this round must ask for. The first round asks for all of them.
    let mut to_ask: Vec<usize> = (0..outline.chambers.len()).collect();

    for proof_attempt in 0..=MAX_PROOF_RETRIES {
        if cancelled() {
            return Err(GenError::Cancelled);
        }
        if proof_attempt > 0 {
            progress(Progress::Retrying {
                attempt: proof_attempt,
                why: proof_note.clone().unwrap_or_default(),
            });
        }

        // Stage two: each chamber's contents.
        for &index in &to_ask {
            let chamber = &outline.chambers[index];
            progress(Progress::Chamber {
                index,
                total: outline.chambers.len(),
                name: chamber.name.clone(),
            });
            let mut accepted = None;
            let mut why = String::new();
            for attempt in 0..=MAX_CHAMBER_RETRIES {
                let note = match (attempt, proof_attempt, &proof_note) {
                    (0, 0, _) => None,
                    (0, _, Some(p)) => Some(p.clone()),
                    _ => Some(why.clone()),
                };
                let user = chamber_prompt(params, &outline, index, note.as_deref());
                let reply = ask(client, user)?;
                match parse_entities(&reply, params.extent) {
                    Ok((list, left_out)) => {
                        accepted = Some(list);
                        skipped_by_chamber[index] = left_out
                            .into_iter()
                            .map(|note| format!("in \"{}\": {note}", chamber.name))
                            .collect();
                        break;
                    }
                    Err(problem) => why = problem,
                }
            }
            let list = accepted.ok_or_else(|| {
                GenError::Invalid(format!(
                    "the chamber \"{}\" never came back usable — {why}.",
                    chamber.name
                ))
            })?;
            chamber_entities[index] = Some(list);
        }

        let entities: Vec<LevelEntity> =
            chamber_entities.iter().flatten().flatten().cloned().collect();
        let skipped: Vec<String> = skipped_by_chamber.iter().flatten().cloned().collect();

        let level = LevelData {
            version: "1.0".to_string(),
            background: None,
            background_size: None,
            level_size: Some([params.extent.x, params.extent.y]),
            spawn: Some([outline.spawn.x, outline.spawn.y]),
            exit: Some([outline.exit.x, outline.exit.y]),
            entities,
        };

        // Stage three: prove it.
        progress(Progress::Proving);
        if cancelled() {
            return Err(GenError::Cancelled);
        }
        let problem = problem_for(&level, outline.spawn, outline.exit, params.extent);
        match traversal::prove(&problem) {
            Verdict::Reachable(route) if traversal::route_arrives(&problem, &route) => {
                return Ok(Generated {
                    level,
                    repairs: vec![],
                    skipped,
                    route_moves: route.moves.len(),
                    route_secs: route.duration(),
                    chambers: outline.chambers.iter().map(|c| c.name.clone()).collect(),
                })
            }
            // A proof that does not replay would not be a proof. This cannot happen while
            // the prover and the replay drive the same simulation, which is the point of
            // checking: if it ever did, the level must not be delivered.
            Verdict::Reachable(_) => {
                proof_note = Some(
                    "The route the search found did not survive being replayed."
                        .to_string(),
                );
                if proof_attempt == MAX_PROOF_RETRIES {
                    return Err(GenError::Unprovable(proof_note.unwrap_or_default()));
                }
            }
            Verdict::Unreachable(frontier) => {
                proof_note = Some(traversal::explain(&frontier, outline.exit));
                // Ask again only for the chamber the route dies in — that is where the way
                // on has to be built. When the failure is not inside any chamber, there is
                // nothing to single out, so the whole area is re-asked.
                to_ask = match chamber_containing(&outline, frontier.deepest) {
                    Some(index) => vec![index],
                    None => (0..outline.chambers.len()).collect(),
                };
                for &index in &to_ask {
                    chamber_entities[index] = None;
                    skipped_by_chamber[index].clear();
                }
                // On the last round, stop asking and repair what the model wrote.
                if proof_attempt == MAX_PROOF_RETRIES {
                    progress(Progress::Repairing);
                    if cancelled() {
                        return Err(GenError::Cancelled);
                    }
                    let repaired = crate::repair::repair(&level, &problem)?;
                    return Ok(Generated {
                        level: repaired.level,
                        repairs: repaired.notes,
                        skipped,
                        route_moves: repaired.route_moves,
                        route_secs: repaired.route_secs,
                        chambers: outline.chambers.iter().map(|c| c.name.clone()).collect(),
                    });
                }
            }
        }
    }

    Err(GenError::Unprovable(proof_note.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_client::ScriptedClient;

    fn extent() -> Vec2 {
        Vec2::new(4000.0, 2000.0)
    }

    // A4 — an invented entity name never reaches the canvas.
    #[test]
    fn an_entity_name_outside_the_toolbox_is_rejected() {
        let reply = r#"{"entities":[{"type":"bitmap","position":[100,100],
                        "bitmap_name":"spike_wall","size":[32,32]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("spike_wall is not a tool");
        assert!(err.contains("spike_wall"), "the offending name is named: {err}");
        assert!(err.contains("coin_tool"), "and the allowed ones are listed: {err}");
    }

    #[test]
    fn a_polygon_type_outside_the_toolbox_is_rejected() {
        let reply = r#"{"entities":[{"type":"polygon","vertices":[[0,0],[10,0],[10,10]],
                        "polygon_type":"lava_tool"}]}"#;
        let err = parse_entities(reply, extent()).expect_err("lava_tool is not a tool");
        assert!(err.contains("lava_tool"), "{err}");
    }

    #[test]
    fn an_invented_entity_kind_is_rejected() {
        let reply = r#"{"entities":[{"type":"circle","position":[1,1]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("there are no circles");
        assert!(err.contains("circle"), "{err}");
    }

    // A5 — structural validation.
    #[test]
    fn a_polygon_with_fewer_than_three_vertices_is_rejected() {
        let reply = r#"{"entities":[{"type":"polygon","vertices":[[0,0],[10,0]],
                        "polygon_type":"wall_tool"}]}"#;
        let err = parse_entities(reply, extent()).expect_err("two points is not a polygon");
        assert!(err.contains("at least 3"), "{err}");
    }

    #[test]
    fn a_non_finite_coordinate_is_rejected() {
        // JSON has no NaN literal, so this arrives as a string, which is equally not a number.
        let reply = r#"{"entities":[{"type":"polygon","vertices":[[0,0],["NaN",0],[10,10]],
                        "polygon_type":"wall_tool"}]}"#;
        assert!(parse_entities(reply, extent()).is_err());
        let huge = format!(
            r#"{{"entities":[{{"type":"polygon","vertices":[[0,0],[{},0],[10,10]],
               "polygon_type":"wall_tool"}}]}}"#,
            1e30
        );
        let err = parse_entities(&huge, extent()).expect_err("outside the level");
        assert!(err.contains("outside the level"), "{err}");
    }

    #[test]
    fn a_coordinate_outside_the_level_is_rejected() {
        let reply = r#"{"entities":[{"type":"polygon","vertices":[[0,0],[9000,0],[10,10]],
                        "polygon_type":"wall_tool"}]}"#;
        let err = parse_entities(reply, extent()).expect_err("9000 is past the 4000 wide level");
        assert!(err.contains("outside the level"), "{err}");
    }

    #[test]
    fn a_non_positive_bitmap_size_is_rejected() {
        for size in ["[0,64]", "[64,0]", "[-32,32]"] {
            let reply = format!(
                r#"{{"entities":[{{"type":"bitmap","position":[100,100],
                    "bitmap_name":"coin_tool","size":{size}}}]}}"#
            );
            let err = parse_entities(&reply, extent())
                .expect_err(&format!("size {size} must be rejected"));
            assert!(
                err.contains("greater than zero"),
                "the reason names the problem for size {size}: {err}"
            );
        }
    }

    #[test]
    fn a_rope_of_the_wrong_thickness_is_rejected() {
        let reply = r#"{"entities":[{"type":"bitmap","position":[100,100],
                        "bitmap_name":"rope_tool","size":[20,300]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("a rope is always 6 thick");
        assert!(err.contains("thickness"), "{err}");
    }

    #[test]
    fn a_rope_shorter_than_sixty_is_rejected() {
        let reply = r#"{"entities":[{"type":"bitmap","position":[100,100],
                        "bitmap_name":"rope_tool","size":[6,40]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("40 is under the 60 px minimum");
        assert!(err.contains("at least 60"), "{err}");

        let ok = r#"{"entities":[{"type":"bitmap","position":[100,100],
                     "bitmap_name":"rope_tool","size":[6,300]}]}"#;
        assert_eq!(parse_entities(ok, extent()).expect("300 is fine").0.len(), 1);
    }

    // The second live run failed here: the model wrote its points as objects rather than
    // arrays, and kept doing so through every retry, because to it the two say the same
    // thing. Refusing an equivalent spelling only burns inference rounds, so every spelling
    // below is accepted — and everything downstream, the vocabulary check, the bounds check
    // and the route proof, is unchanged.
    #[test]
    fn a_point_written_as_an_object_is_accepted_as_readily_as_an_array() {
        let cases = [
            r#"{"type":"bitmap","position":{"x":100,"y":200},"bitmap_name":"coin_tool","size":[64,64]}"#,
            r#"{"type":"bitmap","position":[100,200],"bitmap_name":"coin_tool","size":{"x":64,"y":64}}"#,
            r#"{"type":"bitmap","position":{"x":100,"y":200},"bitmap_name":"coin_tool","size":{"width":64,"height":64}}"#,
            // Numbers as strings, which models do surprisingly often.
            r#"{"type":"bitmap","position":["100","200"],"bitmap_name":"coin_tool","size":[64,64]}"#,
            // The numbers put straight on the entity, with no "position" at all.
            r#"{"type":"bitmap","x":100,"y":200,"bitmap_name":"coin_tool","size":[64,64]}"#,
            // A pair wrapped in another array, which is what a real model actually wrote.
            r#"{"type":"bitmap","position":[[100,200]],"bitmap_name":"coin_tool","size":[64,64]}"#,
            // A longer array: the first two are the point.
            r#"{"type":"bitmap","position":[100,200,0],"bitmap_name":"coin_tool","size":[64,64]}"#,
        ];
        for case in cases {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let (list, _) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("{case} should be accepted: {e}"));
            match &list[0] {
                LevelEntity::Bitmap { position, size, .. } => {
                    assert_eq!(position[0], 100.0, "case {case}");
                    assert_eq!(position[1], 200.0, "case {case}");
                    assert_eq!(size[0], 64.0, "case {case}");
                    assert_eq!(size[1], 64.0, "case {case}");
                }
                other => panic!("expected a bitmap, got {other:?}"),
            }
        }
    }

    // The third live run failed here: a bitmap named its sprite under some other key, again
    // through every retry. The prompt shows `bitmap_name`; a model paraphrasing rather than
    // copying reaches for `name`, `sprite` or the `type` field itself.
    #[test]
    fn a_sprite_named_under_any_reasonable_key_is_understood() {
        let cases = [
            r#"{"type":"bitmap","position":[100,200],"bitmap_name":"orc_tool","size":[128,128]}"#,
            r#"{"type":"bitmap","position":[100,200],"name":"orc_tool","size":[128,128]}"#,
            r#"{"type":"bitmap","position":[100,200],"sprite":"orc_tool","size":[128,128]}"#,
            r#"{"type":"bitmap","position":[100,200],"asset":"orc_tool","size":[128,128]}"#,
            // The tool name written straight into `type`.
            r#"{"type":"orc_tool","position":[100,200],"size":[128,128]}"#,
            // No `type` at all: the contents say what it is.
            r#"{"position":[100,200],"name":"orc_tool","size":[128,128]}"#,
        ];
        for case in cases {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let (list, _) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("{case} should be understood: {e}"));
            match &list[0] {
                LevelEntity::Bitmap { bitmap_name, .. } => {
                    assert_eq!(bitmap_name, "orc_tool", "case {case}")
                }
                other => panic!("expected a bitmap, got {other:?}"),
            }
        }
    }

    // A model that gives both a human label and the real answer must be read for the answer.
    // Taking whichever key came first read the label and missed the name, which condemned an
    // entity that had named itself perfectly well — and, when it also carried vertices, made
    // a hazard into a platform, the one mistake the route proof can never catch because added
    // floor only ever makes a route easier.
    #[test]
    fn a_label_alongside_a_real_sprite_name_does_not_hide_it() {
        let cases = [
            r#"{"type":"bitmap","name":"spiked pit","sprite":"death_trap_tool","position":[400,900],"size":[48,48]}"#,
            r#"{"type":"bitmap","kind":"hazard","name":"death_trap_tool","position":[400,900],"size":[48,48]}"#,
            r#"{"name":"the long drop","asset":"death_trap_tool","position":[400,900],"size":[48,48]}"#,
        ];
        for case in cases {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let (list, skipped) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("{case} names its sprite: {e}"));
            assert!(skipped.is_empty(), "nothing should be left out of {case}: {skipped:?}");
            match &list[0] {
                LevelEntity::Bitmap { bitmap_name, .. } => {
                    assert_eq!(bitmap_name, "death_trap_tool", "case {case}")
                }
                other => panic!("expected the death trap for {case}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_labelled_hazard_carrying_vertices_is_still_a_hazard_not_a_platform() {
        // The dangerous half: with vertices present, missing the sprite name turned a death
        // trap into solid floor, silently, and the prover could not notice.
        let reply = r#"{"entities":[{"type":"bitmap","name":"spiked pit",
                        "sprite":"death_trap_tool","position":[400,900],"size":[48,48],
                        "vertices":[[380,880],[420,880],[420,920]]}]}"#;
        let (list, _) = parse_entities(reply, extent()).expect("it names a sprite");
        match &list[0] {
            LevelEntity::Bitmap { bitmap_name, .. } => assert_eq!(bitmap_name, "death_trap_tool"),
            LevelEntity::Polygon { .. } => {
                panic!("a death trap became solid geometry — the route proof cannot catch this")
            }
        }
    }

    #[test]
    fn a_polygon_type_alongside_a_label_is_not_downgraded() {
        // The same shape on the polygon side: a label under an earlier key must not cost the
        // polygon its climbability.
        let reply = r#"{"entities":[{"vertices":[[0,900],[800,900],[800,1000]],
                        "kind":"decoration","tool":"wall_tool"}]}"#;
        let (list, _) = parse_entities(reply, extent()).expect("it names a real type");
        match &list[0] {
            LevelEntity::Polygon { polygon_type, .. } => {
                assert_eq!(polygon_type.as_deref(), Some("wall_tool"), "climbability kept")
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn a_polygon_type_under_another_key_or_in_the_type_field_is_understood() {
        let cases = [
            r#"{"type":"polygon","polygon_type":"wall_tool","vertices":[[0,0],[10,0],[10,10]]}"#,
            r#"{"type":"polygon","kind":"wall_tool","vertices":[[0,0],[10,0],[10,10]]}"#,
            r#"{"type":"wall_tool","vertices":[[0,0],[10,0],[10,10]]}"#,
            r#"{"vertices":[[0,0],[10,0],[10,10]],"tool":"wall_tool"}"#,
        ];
        for case in cases {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let (list, _) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("{case} should be understood: {e}"));
            match &list[0] {
                LevelEntity::Polygon { polygon_type, .. } => {
                    assert_eq!(polygon_type.as_deref(), Some("wall_tool"), "case {case}")
                }
                other => panic!("expected a polygon, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_label_on_a_polygon_is_not_mistaken_for_its_type() {
        // "name" is one of the keys a type may hide under, but a chamber's own label is not
        // a mistake — it must be passed over, not rejected.
        let reply = r#"{"entities":[{"type":"polygon","name":"north floor",
                        "vertices":[[0,0],[10,0],[10,10]]}]}"#;
        let (list, _) = parse_entities(reply, extent()).expect("a label is not an error");
        match &list[0] {
            LevelEntity::Polygon { polygon_type, .. } => {
                assert_eq!(polygon_type.as_deref(), Some("polygon_tool"), "it falls back");
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    // A real model wrote an entity carrying only a position and a type — no sprite at all.
    // Nothing can guess what it meant. Failing the chamber would throw away every other
    // entity in it and the minutes that produced them, so it is left out and reported, and
    // whether it mattered is a question the route proof answers a moment later.
    #[test]
    fn an_entity_that_never_says_which_sprite_it_is_is_left_out_and_reported() {
        let reply = r#"{"entities":[
            {"type":"polygon","polygon_type":"wall_tool","vertices":[[0,900],[800,900],[800,1000]]},
            {"type":"bitmap","position":[100,200],"size":[64,64],"colour":"red","note":"a torch"},
            {"type":"bitmap","position":[300,200],"bitmap_name":"coin_tool","size":[64,64]}
        ]}"#;
        let (list, skipped) = parse_entities(reply, extent()).expect("the rest is still usable");
        assert_eq!(list.len(), 2, "the floor and the coin survive");
        assert_eq!(skipped.len(), 1, "and the nameless one is left out");
        assert!(skipped[0].contains("never which sprite"), "{:?}", skipped[0]);
        assert!(skipped[0].contains("\"colour\""), "the keys it did have are named: {:?}", skipped[0]);
        assert!(skipped[0].contains("\"note\""), "{:?}", skipped[0]);
    }

    // Collision geometry mislabelled as a bitmap must not be dropped as a decoration: the
    // vertices say what it is, whatever the label claims.
    #[test]
    fn an_entity_carrying_vertices_is_a_polygon_whatever_it_calls_itself() {
        for case in [
            r#"{"type":"bitmap","polygon_type":"wall_tool","vertices":[[0,900],[800,900],[800,1000]]}"#,
            r#"{"polygon_type":"wall_tool","vertices":[[0,900],[800,900],[800,1000]]}"#,
        ] {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let (list, skipped) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("{case} is a polygon: {e}"));
            assert!(skipped.is_empty(), "a floor must not be dropped as a decoration: {skipped:?}");
            match &list[0] {
                LevelEntity::Polygon { vertices, polygon_type, .. } => {
                    assert_eq!(vertices.len(), 3, "case {case}");
                    assert_eq!(polygon_type.as_deref(), Some("wall_tool"), "case {case}");
                }
                other => panic!("expected a polygon for {case}, got {other:?}"),
            }
        }
    }

    // The other half of that rule, and the more important half: an entity that names a real
    // sprite is taken at its word even when it also carries vertices. Turning a death trap
    // into a platform is the one mistake the route proof can never catch, because added
    // floor makes a route easier rather than harder — it would ship silently.
    #[test]
    fn a_sprite_that_names_itself_is_not_turned_into_geometry_by_stray_vertices() {
        let reply = r#"{"entities":[
            {"type":"death_trap_tool","position":[400,900],"size":[48,48],
             "vertices":[[0,0],[10,0],[10,10]]},
            {"type":"bitmap","bitmap_name":"coin_tool","position":[600,900],"size":[64,64],
             "vertices":[[0,0],[10,0],[10,10]]}
        ]}"#;
        let (list, skipped) = parse_entities(reply, extent()).expect("both are sprites");
        assert!(skipped.is_empty(), "{skipped:?}");
        let names: Vec<&str> = list
            .iter()
            .map(|e| match e {
                LevelEntity::Bitmap { bitmap_name, .. } => bitmap_name.as_str(),
                LevelEntity::Polygon { .. } => panic!("a named sprite became solid geometry"),
            })
            .collect();
        assert_eq!(names, vec!["death_trap_tool", "coin_tool"]);
    }

    // A model that writes one entity shape with nulls in the fields it is not using — a coin
    // carrying "vertices": null — must not have its whole chamber condemned.
    #[test]
    fn an_empty_or_null_vertices_field_on_a_sprite_is_ignored() {
        for vertices in ["null", "[]"] {
            let reply = format!(
                r#"{{"entities":[{{"type":"bitmap","bitmap_name":"coin_tool",
                   "position":[100,100],"size":[64,64],"vertices":{vertices}}}]}}"#
            );
            let (list, skipped) = parse_entities(&reply, extent())
                .unwrap_or_else(|e| panic!("vertices {vertices} should be ignored: {e}"));
            assert!(skipped.is_empty(), "{skipped:?}");
            match &list[0] {
                LevelEntity::Bitmap { bitmap_name, .. } => assert_eq!(bitmap_name, "coin_tool"),
                other => panic!("expected the coin, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_invented_name_is_still_reported_even_on_an_entity_carrying_vertices() {
        // Claiming to be a sprite is enough to be held to the vocabulary, vertices or not.
        let reply = r#"{"entities":[{"type":"bitmap","bitmap_name":"spike_wall",
                        "position":[100,100],"size":[64,64],
                        "vertices":[[0,0],[10,0],[10,10]]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("spike_wall is not a tool");
        assert!(err.contains("spike_wall"), "{err}");
    }

    #[test]
    fn a_reply_where_nothing_at_all_is_usable_is_still_an_error() {
        // Dropping the unusable must not quietly become accepting an empty chamber.
        let reply = r#"{"entities":[
            {"type":"bitmap","position":[100,200]},
            {"type":"bitmap","position":[300,200]}
        ]}"#;
        let err = parse_entities(reply, extent()).expect_err("nothing survived, so ask again");
        assert!(err.contains("none of the 2 entities"), "{err}");
    }

    #[test]
    fn a_sprite_with_no_size_gets_the_size_that_sprite_always_has() {
        let reply = r#"{"entities":[
            {"type":"bitmap","position":[100,200],"bitmap_name":"orc_tool"},
            {"type":"bitmap","position":[300,200],"bitmap_name":"coin_tool"},
            {"type":"bitmap","position":[500,200],"bitmap_name":"rope_tool"}
        ]}"#;
        let (list, skipped) = parse_entities(reply, extent()).expect("a missing size is not fatal");
        assert!(skipped.is_empty(), "nothing needed leaving out");
        let sizes: Vec<[f32; 2]> = list
            .iter()
            .map(|e| match e {
                LevelEntity::Bitmap { size, .. } => *size,
                other => panic!("expected a bitmap, got {other:?}"),
            })
            .collect();
        assert_eq!(sizes[0], [128.0, 128.0], "an orc");
        assert_eq!(sizes[1], [64.0, 64.0], "a coin");
        assert_eq!(sizes[2], [ROPE_THICKNESS, 300.0], "a rope, at the thickness ropes are");
    }

    #[test]
    fn an_invented_sprite_name_is_still_rejected_wherever_it_is_written() {
        for case in [
            r#"{"type":"bitmap","position":[1,1],"bitmap_name":"spike_wall","size":[9,9]}"#,
            r#"{"type":"bitmap","position":[1,1],"name":"spike_wall","size":[9,9]}"#,
        ] {
            let reply = format!(r#"{{"entities":[{case}]}}"#);
            let err = parse_entities(&reply, extent()).expect_err("spike_wall is not a tool");
            assert!(err.contains("spike_wall"), "the offender is named: {err}");
            assert!(err.contains("coin_tool"), "and the allowed ones listed: {err}");
        }
    }

    #[test]
    fn a_vertex_list_wrapped_in_another_list_is_unwrapped() {
        let reply = r#"{"entities":[{"type":"polygon","polygon_type":"wall_tool",
                        "vertices":[[[0,900],[800,900],[800,1000]]]}]}"#;
        let (list, _) = parse_entities(reply, extent()).expect("one level of wrapping is unwrapped");
        match &list[0] {
            LevelEntity::Polygon { vertices, .. } => {
                assert_eq!(vertices.len(), 3);
                assert_eq!(vertices[1], [800.0, 900.0]);
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn two_points_where_one_was_wanted_is_still_an_error() {
        // Unwrapping a single wrapped pair must not turn into silently taking the first of
        // several, which would discard something the model meant.
        let reply = r#"{"entities":[{"type":"bitmap","position":[[100,200],[300,400]],
                        "bitmap_name":"coin_tool","size":[64,64]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("two points is not a point");
        assert!(err.contains("not a pair of numbers"), "{err}");
    }

    #[test]
    fn polygon_vertices_written_as_objects_are_accepted() {
        let reply = r#"{"entities":[{"type":"polygon","polygon_type":"wall_tool","vertices":[
            {"x":0,"y":900},{"x":800,"y":900},{"x":800,"y":1000}]}]}"#;
        let (list, _) = parse_entities(reply, extent()).expect("object vertices are a point each");
        match &list[0] {
            LevelEntity::Polygon { vertices, .. } => {
                assert_eq!(vertices.len(), 3);
                assert_eq!(vertices[1], [800.0, 900.0]);
            }
            other => panic!("expected a polygon, got {other:?}"),
        }
    }

    #[test]
    fn an_outline_written_with_objects_throughout_is_accepted() {
        let reply = r#"{"chambers":[
            {"name":"West Entry Hall","rect":{"x":0,"y":0,"width":2000,"height":2000},"role":"in"},
            {"name":"The Deep","rect":[2000,0,2000,2000],"role":"on"}],
            "connections":[{"from":0,"to":1,"at":{"x":2000,"y":1200}}],
            "spawn":{"x":300,"y":1100},"exit":[3600,1155]}"#;
        let outline = parse_outline(reply, extent()).expect("objects throughout are fine");
        assert_eq!(outline.chambers[0].rect, [0.0, 0.0, 2000.0, 2000.0]);
        assert_eq!(outline.spawn, Pos2::new(300.0, 1100.0));
        assert_eq!(outline.connections[0].at, Pos2::new(2000.0, 1200.0));
    }

    #[test]
    fn something_that_is_not_a_point_at_all_says_what_it_actually_was() {
        // The old message said only that a position "must be [x, y]", which told neither the
        // model on retry nor the user in a bug report what had actually arrived.
        let reply = r#"{"entities":[{"type":"bitmap","position":"middle of the room",
                        "bitmap_name":"coin_tool","size":[64,64]}]}"#;
        let err = parse_entities(reply, extent()).expect_err("a sentence is not a point");
        assert!(err.contains("middle of the room"), "the error shows what arrived: {err}");
        assert!(err.contains("[x, y]"), "and what was wanted: {err}");
    }

    #[test]
    fn a_valid_chamber_parses_into_entities() {
        let reply = r#"Here you go:
        ```json
        {"entities":[
          {"type":"polygon","vertices":[[0,900],[800,900],[800,1000],[0,1000]],"polygon_type":"wall_tool"},
          {"type":"bitmap","position":[300,800],"bitmap_name":"coin_tool","size":[64,64]}
        ]}
        ```"#;
        let (list, _) = parse_entities(reply, extent()).expect("prose and a fence are tolerated");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn an_outline_parses_with_its_chambers_spawn_and_exit() {
        let reply = r#"{"chambers":[
            {"name":"The Cistern","rect":[0,0,2000,2000],"role":"opening"},
            {"name":"The Deep","rect":[2000,0,2000,2000],"role":"finish"}],
            "connections":[{"from":0,"to":1,"at":[2000,1500]}],
            "spawn":[200,1500],"exit":[3600,1500]}"#;
        let outline = parse_outline(reply, extent()).expect("parses");
        assert_eq!(outline.chambers.len(), 2);
        assert_eq!(outline.chambers[0].name, "The Cistern");
        assert_eq!(outline.spawn, Pos2::new(200.0, 1500.0));
        assert_eq!(outline.exit, Pos2::new(3600.0, 1500.0));
        assert_eq!(
            outline.connections,
            vec![Connection { from: 0, to: 1, at: Pos2::new(2000.0, 1500.0) }]
        );
    }

    // A24 — the outline must say where its chambers meet.
    #[test]
    fn a_multi_chamber_outline_with_no_connections_is_rejected() {
        let reply = r#"{"chambers":[
            {"name":"a","rect":[0,0,2000,2000],"role":"r"},
            {"name":"b","rect":[2000,0,2000,2000],"role":"r"}],
            "spawn":[200,1500],"exit":[3600,1500]}"#;
        let err = parse_outline(reply, extent()).expect_err("chambers that never join are useless");
        assert!(err.contains("no connections"), "{err}");
    }

    #[test]
    fn a_single_chamber_outline_needs_no_connections() {
        let reply = r#"{"chambers":[{"name":"only","rect":[0,0,2000,2000],"role":"r"}],
                        "spawn":[200,1500],"exit":[1800,1500]}"#;
        let outline = parse_outline(reply, extent()).expect("one chamber joins nothing");
        assert!(outline.connections.is_empty());
    }

    #[test]
    fn a_nonsense_connection_is_rejected() {
        let cases = [
            (r#""connections":[{"from":0,"to":9,"at":[10,10]}]"#, "only 2 chambers"),
            (r#""connections":[{"from":0,"to":0,"at":[10,10]}]"#, "to itself"),
            (r#""connections":[{"from":0,"to":1,"at":[99999,10]}]"#, "outside the level"),
            (r#""connections":[{"from":0,"to":1}]"#, r#"has no "at""#),
        ];
        for (connections, expected) in cases {
            let reply = format!(
                r#"{{"chambers":[{{"name":"a","rect":[0,0,2000,2000],"role":"r"}},
                    {{"name":"b","rect":[2000,0,2000,2000],"role":"r"}}],
                    {connections},"spawn":[200,1500],"exit":[3600,1500]}}"#
            );
            let err = parse_outline(&reply, extent())
                .expect_err(&format!("{connections} must be rejected"));
            assert!(
                err.contains(expected),
                "the reason should mention {expected:?} for {connections}: {err}"
            );
        }
    }

    #[test]
    fn an_area_with_an_unreachable_chamber_is_rejected() {
        // Three chambers, but the third joins nothing: there is no route to it.
        let reply = r#"{"chambers":[
            {"name":"a","rect":[0,0,1000,2000],"role":"r"},
            {"name":"b","rect":[1000,0,1000,2000],"role":"r"},
            {"name":"Stranded","rect":[2000,0,1000,2000],"role":"r"}],
            "connections":[{"from":0,"to":1,"at":[1000,1500]}],
            "spawn":[200,1500],"exit":[2500,1500]}"#;
        let err = parse_outline(reply, extent()).expect_err("a room with no way in is not a level");
        assert!(err.contains("Stranded"), "the unreachable chamber is named: {err}");
        assert!(err.contains("no route through the area"), "{err}");
    }

    #[test]
    fn two_passages_between_the_same_pair_are_allowed() {
        // An upper and a lower route between the same two chambers is ordinary design, and
        // rejecting it cost a whole outline retry for nothing.
        let reply = r#"{"chambers":[
            {"name":"a","rect":[0,0,2000,2000],"role":"r"},
            {"name":"b","rect":[2000,0,2000,2000],"role":"r"}],
            "connections":[{"from":0,"to":1,"at":[2000,1200]},
                           {"from":1,"to":0,"at":[2000,800]}],
            "spawn":[200,1500],"exit":[3600,1500]}"#;
        let outline = parse_outline(reply, extent()).expect("two passages are fine");
        assert_eq!(outline.connections.len(), 2);
        // And the chamber is told to floor both of them.
        assert_eq!(outline.doorways(0).len(), 2);
    }

    #[test]
    fn the_same_doorway_stated_twice_is_rejected() {
        let reply = r#"{"chambers":[
            {"name":"a","rect":[0,0,2000,2000],"role":"r"},
            {"name":"b","rect":[2000,0,2000,2000],"role":"r"}],
            "connections":[{"from":0,"to":1,"at":[2000,1200]},
                           {"from":1,"to":0,"at":[2000,1200]}],
            "spawn":[200,1500],"exit":[3600,1500]}"#;
        let err = parse_outline(reply, extent()).expect_err("the same doorway twice");
        assert!(err.contains("repeats a doorway"), "{err}");
    }

    #[test]
    fn a_chamber_rect_that_does_not_fit_the_level_is_rejected() {
        let reply = r#"{"chambers":[{"name":"huge","rect":[0,0,99999,2000],"role":"r"}],
                        "spawn":[200,1500],"exit":[1800,1500]}"#;
        let err = parse_outline(reply, extent()).expect_err("it does not fit");
        assert!(err.contains("does not fit"), "{err}");

        // And a value that is finite as an f64 but infinite once cast to f32.
        let overflow = r#"{"chambers":[{"name":"huge","rect":[0,0,1e40,2000],"role":"r"}],
                           "spawn":[200,1500],"exit":[1800,1500]}"#;
        let err = parse_outline(overflow, extent()).expect_err("1e40 is not a usable width");
        assert!(err.contains("not a usable number"), "{err}");
    }

    #[test]
    fn a_chamber_is_told_every_point_it_must_floor() {
        let outline = Outline {
            chambers: vec![
                ChamberPlan { name: "Near".into(), rect: [0.0, 0.0, 2000.0, 2000.0], role: "start".into() },
                ChamberPlan { name: "Middle".into(), rect: [2000.0, 0.0, 1000.0, 2000.0], role: "on".into() },
                ChamberPlan { name: "Far".into(), rect: [3000.0, 0.0, 1000.0, 2000.0], role: "end".into() },
            ],
            connections: vec![
                Connection { from: 0, to: 1, at: Pos2::new(2000.0, 1500.0) },
                Connection { from: 1, to: 2, at: Pos2::new(3000.0, 1500.0) },
            ],
            spawn: Pos2::new(200.0, 1400.0),
            exit: Pos2::new(3800.0, 1455.0),
        };
        // The middle chamber owns neither the spawn nor the exit, but joins both neighbours.
        assert_eq!(
            outline.doorways(1),
            vec![(Pos2::new(2000.0, 1500.0), 0), (Pos2::new(3000.0, 1500.0), 2)]
        );
        assert_eq!(outline.doorways(0), vec![(Pos2::new(2000.0, 1500.0), 1)]);
        let params = GenerationParams { prompt: "p".into(), extent: Vec2::new(4000.0, 2000.0), chambers: 3 };
        let text = chamber_prompt(&params, &outline, 1, None);
        assert!(text.contains("(2000, 1500)"), "the prompt names the first doorway: {text}");
        assert!(text.contains("(3000, 1500)"), "and the second: {text}");
        assert!(text.contains("\"Near\""), "and which chamber is on the other side: {text}");
        assert!(text.contains("\"Far\""), "got: {text}");
        assert!(text.contains("continuous solid"), "and what it must do about them: {text}");

        // The first chamber is told about the spawn as well as its one doorway.
        let first = chamber_prompt(&params, &outline, 0, None);
        assert!(first.contains("The run begins at (200, 1400)"), "got: {first}");
        assert!(first.contains("(2000, 1500)"), "got: {first}");
        assert!(!first.contains("(3000, 1500)"), "and not about a doorway it does not own");
    }

    #[test]
    fn the_outline_request_asks_for_connections_and_says_why_they_matter() {
        let text = outline_prompt(&GenerationParams::default());
        assert!(text.contains("\"connections\""), "got: {text}");
        assert!(text.contains("\"from\":0,\"to\":1"), "the shape is shown: {text}");
        assert!(
            text.contains("filled in one at a time"),
            "the model is told why it must say where chambers join: {text}"
        );
        assert!(
            text.contains("EVERY chamber must be reachable from the first one"),
            "the prompt must state the rule the validator enforces: {text}"
        );
        assert!(
            text.contains("more than one connection between the same pair"),
            "and that two passages are allowed: {text}"
        );
    }

    #[test]
    fn an_outline_whose_spawn_is_outside_the_level_is_rejected() {
        let reply = r#"{"chambers":[{"name":"a","rect":[0,0,100,100],"role":"r"}],
                        "spawn":[99999,10],"exit":[10,10]}"#;
        let err = parse_outline(reply, extent()).expect_err("the spawn is off the map");
        assert!(err.contains("outside the level"), "{err}");
    }

    #[test]
    fn the_system_prompt_states_the_characters_actual_reach() {
        let text = system_prompt(extent());
        for expected in [
            &format!("{:.0}", cfg::JUMP_HEIGHT),
            &format!("{:.0}", cfg::DASH_DISTANCE),
            &format!("{:.0}", cfg::WALL_JUMP_HEIGHT),
            &format!("{:.0}", cfg::CLIMB_SPEED),
        ] {
            assert!(text.contains(expected.as_str()), "the prompt omits {expected}");
        }
        assert!(text.contains("NO double jump"), "the prompt must rule out a double jump");
        assert!(text.contains("y DOWNWARD"), "the prompt must state the Y direction");
        for name in BITMAP_NAMES.iter().chain(POLYGON_TYPES.iter()) {
            assert!(text.contains(name), "the prompt omits {name}");
        }
    }

    // A cancelled run stops without delivering anything.
    #[test]
    fn a_cancelled_run_stops_and_delivers_nothing() {
        let cancel = Arc::new(AtomicBool::new(true));
        let mut client = ScriptedClient::always("{}");
        let err = generate(
            &mut client,
            &GenerationParams::default(),
            &mut |_| {},
            &cancel,
        )
        .expect_err("cancelled");
        assert_eq!(err, GenError::Cancelled);
        assert!(client.requests.is_empty(), "a cancelled run must not even ask");
    }

    /// A recorded transcript: what a model that does its job replies, for the whole run.
    /// Two chambers side by side, each with a floor, joined into one walkable area.
    fn a_good_transcript() -> Vec<Result<String, ModelError>> {
        vec![
            Ok(r#"{"chambers":[
                {"name":"The Cistern","rect":[0,0,2000,2000],"role":"the way in"},
                {"name":"The Drowned Hall","rect":[2000,0,2000,2000],"role":"the way on"}],
                "connections":[{"from":0,"to":1,"at":[2000,1200]}],
                "spawn":[300,1100],"exit":[3600,1155]}"#
                .to_string()),
            Ok(r#"Here is the first chamber:
            ```json
            {"entities":[
              {"type":"polygon","vertices":[[0,1200],[2000,1200],[2000,1400],[0,1400]],
               "polygon_type":"blocker_tool"},
              {"type":"bitmap","position":[900,1120],"bitmap_name":"coin_tool","size":[64,64]},
              {"type":"bitmap","position":[1400,1150],"bitmap_name":"death_trap_tool","size":[48,48]}
            ]}
            ```"#
                .to_string()),
            Ok(r#"{"entities":[
              {"type":"polygon","vertices":[[2000,1200],[4000,1200],[4000,1400],[2000,1400]],
               "polygon_type":"blocker_tool"},
              {"type":"polygon","vertices":[[2600,700],[2700,700],[2700,1200],[2600,1200]],
               "polygon_type":"wall_tool"},
              {"type":"bitmap","position":[3000,1080],"bitmap_name":"orc_tool","size":[128,128]},
              {"type":"bitmap","position":[3300,600],"bitmap_name":"rope_tool","size":[6,300]}
            ]}"#
                .to_string()),
        ]
    }

    // A18 — the whole pipeline, end to end, with no model server anywhere near it.
    #[test]
    fn an_end_to_end_run_from_a_recorded_transcript_produces_a_proven_level() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut client = ScriptedClient::new(a_good_transcript());
        let params = GenerationParams {
            prompt: "a flooded cistern".to_string(),
            extent: Vec2::new(4000.0, 2000.0),
            chambers: 2,
        };
        let mut stages = Vec::new();

        let generated = generate(&mut client, &params, &mut |p| stages.push(p), &cancel)
            .expect("the transcript describes a perfectly good level");

        // Every entity is one the editor can actually place.
        assert_eq!(generated.level.entities.len(), 7);
        for entity in &generated.level.entities {
            match entity {
                LevelEntity::Polygon { polygon_type, vertices, .. } => {
                    let kind = polygon_type.as_deref().unwrap_or("");
                    assert!(POLYGON_TYPES.contains(&kind), "unknown polygon type {kind:?}");
                    assert!(vertices.len() >= 3);
                }
                LevelEntity::Bitmap { bitmap_name, size, .. } => {
                    assert!(
                        BITMAP_NAMES.contains(&bitmap_name.as_str()),
                        "unknown bitmap {bitmap_name:?}"
                    );
                    assert!(size[0] > 0.0 && size[1] > 0.0);
                }
            }
        }

        // The start, the end and the extent are all recorded in the level itself.
        assert_eq!(generated.level.spawn, Some([300.0, 1100.0]));
        assert_eq!(generated.level.exit, Some([3600.0, 1155.0]));
        assert_eq!(
            generated.level.level_size,
            Some([params.extent.x, params.extent.y]),
            "the level records the extent it was generated into"
        );

        // It is the model's own work, and the route really is walkable.
        assert!(generated.is_the_models_own(), "nothing needed repairing: {:?}", generated.repairs);
        assert!(generated.route_moves > 0);
        assert_eq!(generated.chambers, vec!["The Cistern", "The Drowned Hall"]);

        let problem = problem_for(
            &generated.level,
            Pos2::new(300.0, 1100.0),
            Pos2::new(3600.0, 1155.0),
            params.extent,
        );
        match traversal::prove(&problem) {
            Verdict::Reachable(route) => {
                let landed = traversal::replay(&problem, &route);
                assert!(
                    (landed - problem.exit).length() <= traversal::ARRIVAL_RADIUS,
                    "the delivered level's route must replay to the exit"
                );
            }
            Verdict::Unreachable(f) => {
                panic!("the delivered level must be walkable: {}", traversal::explain(&f, problem.exit))
            }
        }

        // One request for the outline and one per chamber, each carrying the rules.
        assert_eq!(client.requests.len(), 3, "the outline, then one request per chamber");
        for request in &client.requests {
            assert!(request.json_only, "every request asks for JSON");
            assert!(request.system.contains("y DOWNWARD"), "every request carries the rules");
        }
        assert!(client.requests[1].user.contains("The Cistern"));
        assert!(client.requests[1].user.contains("The run begins at (300, 1100)"));
        assert!(client.requests[2].user.contains("The Drowned Hall"));
        assert!(client.requests[2].user.contains("The route ends at (3600, 1155)"));

        // And the user was kept informed at every stage.
        assert!(stages.iter().any(|s| matches!(s, Progress::Outline)));
        assert!(stages.iter().any(|s| matches!(s, Progress::Chamber { index: 0, .. })));
        assert!(stages.iter().any(|s| matches!(s, Progress::Chamber { index: 1, .. })));
        assert!(stages.iter().any(|s| matches!(s, Progress::Proving)));
        assert!(
            !stages.iter().any(|s| matches!(s, Progress::Repairing)),
            "a level that works is never repaired"
        );
    }

    // A10 and A11 at the level of the whole pipeline: a model that keeps leaving an
    // uncrossable gap is told what is wrong, asked again twice, and then repaired.
    #[test]
    fn an_unprovable_level_is_re_asked_twice_with_the_reason_and_then_repaired() {
        let outline = r#"{"chambers":[
            {"name":"Near","rect":[0,0,2000,2000],"role":"start"},
            {"name":"Far","rect":[2000,0,2000,2000],"role":"finish"}],
            "connections":[{"from":0,"to":1,"at":[2000,1200]}],
            "spawn":[300,1100],"exit":[3600,1155]}"#;
        // The two floors are 1600 px apart: far beyond a running jump with an air dash.
        let near = r#"{"entities":[{"type":"polygon",
            "vertices":[[0,1200],[600,1200],[600,1400],[0,1400]],
            "polygon_type":"blocker_tool"}]}"#;
        let far = r#"{"entities":[{"type":"polygon",
            "vertices":[[2200,1200],[4000,1200],[4000,1400],[2200,1400]],
            "polygon_type":"blocker_tool"}]}"#;

        // The first round asks for both chambers. The route dies at the spawn, which is in
        // the first chamber, so each retry asks for that one alone: two replies, then one,
        // then one.
        let mut replies = vec![Ok(outline.to_string()), Ok(near.to_string()), Ok(far.to_string())];
        for _ in 0..MAX_PROOF_RETRIES {
            replies.push(Ok(near.to_string()));
        }
        let mut client = ScriptedClient::new(replies);
        let params = GenerationParams {
            prompt: "a broken bridge".to_string(),
            extent: Vec2::new(4000.0, 2000.0),
            chambers: 2,
        };
        let mut stages = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        let generated = generate(&mut client, &params, &mut |p| stages.push(p), &cancel)
            .expect("repair must rescue what the model kept getting wrong");

        // One outline, then both chambers, then only the offending chamber on each retry.
        assert_eq!(
            client.requests.len(),
            1 + 2 + MAX_PROOF_RETRIES,
            "a retry must re-ask only the chamber the route dies in, not the whole area — \
             every chamber costs minutes on the hardware this runs on"
        );
        // Which chamber is being asked for is the opening line; the prompt also names the
        // neighbour on the far side of each doorway, so a bare substring search for "Far"
        // would not tell us anything.
        assert!(
            client.requests[3].user.starts_with("Fill the chamber \"Near\""),
            "the retry asks for the chamber the route died in: {}",
            &client.requests[3].user[..120.min(client.requests[3].user.len())]
        );
        assert!(
            !client.requests[3].user.starts_with("Fill the chamber \"Far\""),
            "and not for the one that was fine"
        );

        // The retries carry the actual failure, in terms the model can act on.
        let retry = &client.requests[3].user;
        assert!(
            retry.contains("The nearest foothold beyond it is"),
            "and how far the nearest foothold beyond the dying ledge is: {retry}"
        );
        assert!(
            retry.contains("Your previous attempt was rejected"),
            "the retry tells the model it was rejected: {retry}"
        );
        assert!(
            retry.contains("The route dies on the ledge at"),
            "and names the ledge it died on: {retry}"
        );
        assert!(
            retry.contains("px away"),
            "and how far the exit still is: {retry}"
        );
        assert!(
            retry.contains(&format!("{:.0} px dash", cfg::DASH_DISTANCE)),
            "and what the character can actually clear: {retry}"
        );

        // Two retries, then repair — in that order.
        let retries: Vec<usize> = stages
            .iter()
            .filter_map(|s| match s {
                Progress::Retrying { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .collect();
        assert_eq!(retries, vec![1, 2], "exactly two retries, numbered for the user");
        assert!(stages.iter().any(|s| matches!(s, Progress::Repairing)));

        // And what came back is repaired, proven, and honest about it.
        assert!(!generated.is_the_models_own(), "this level had to be repaired");
        assert!(!generated.repairs.is_empty());
        assert!(generated.repairs[0].contains("added a"), "{:?}", generated.repairs);

        let problem = problem_for(
            &generated.level,
            Pos2::new(300.0, 1100.0),
            Pos2::new(3600.0, 1155.0),
            params.extent,
        );
        assert!(
            traversal::prove(&problem).is_reachable(),
            "the delivered level must pass the same prover that rejected the original"
        );
    }

    // A24 — chambers that meet at their stated connection point produce a level the prover
    // confirms with no repair, and their floors line up there. This is the whole reason the
    // connection points exist: filling chambers in isolation left a step at the seam and
    // repair ran on every level.
    #[test]
    fn chambers_that_meet_at_their_connection_point_need_no_repair() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut client = ScriptedClient::new(a_good_transcript());
        let params = GenerationParams {
            prompt: "a flooded cistern".to_string(),
            extent: Vec2::new(4000.0, 2000.0),
            chambers: 2,
        };
        let mut stages = Vec::new();
        let generated = generate(&mut client, &params, &mut |p| stages.push(p), &cancel)
            .expect("a well-connected area generates");

        assert!(
            generated.is_the_models_own(),
            "a connected area must not need repairing, yet it was: {:?}",
            generated.repairs
        );
        assert!(
            !stages.iter().any(|s| matches!(s, Progress::Repairing)),
            "and repair must never have been entered"
        );
        assert!(
            !stages.iter().any(|s| matches!(s, Progress::Retrying { .. })),
            "nor a retry"
        );

        // The floors on both sides of the doorway are at the same height, so there is no
        // step to climb at the seam.
        let doorway_x = 2000.0_f32;
        let mut tops: Vec<f32> = Vec::new();
        for entity in &generated.level.entities {
            if let LevelEntity::Polygon { vertices, .. } = entity {
                let min_x = vertices.iter().fold(f32::INFINITY, |a, v| a.min(v[0]));
                let max_x = vertices.iter().fold(f32::NEG_INFINITY, |a, v| a.max(v[0]));
                let top = vertices.iter().fold(f32::INFINITY, |a, v| a.min(v[1]));
                if min_x <= doorway_x && max_x >= doorway_x {
                    tops.push(top);
                }
            }
        }
        assert!(tops.len() >= 2, "both chambers must floor the doorway, found {tops:?}");
        let highest = tops.iter().cloned().fold(f32::INFINITY, f32::min);
        let lowest = tops.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            lowest - highest <= cfg::JUMP_HEIGHT,
            "the step at the doorway is {:.0} px, past the {:.0} px the character can jump",
            lowest - highest,
            cfg::JUMP_HEIGHT
        );
    }

    /// A23 — the one check that cannot be faked: a real model, a real request, a real level.
    ///
    /// Ignored by default, so `cargo test` stays entirely offline. It costs money and sends
    /// the prompt to whatever endpoint is configured, so it is run deliberately or not at
    /// all. Everything it asserts, the offline tests also assert against transcripts; what
    /// only this can tell us is whether a real model's reply survives contact with the
    /// parsers, the validator and the route proof — which is exactly where every failure so
    /// far has been.
    ///
    ///     OPENAI_API_KEY=... cargo test a23_live -- --ignored --nocapture
    ///
    /// Override any of these to try a different combination:
    ///     LEVEL_GEN_MODEL     (default: the editor's own DEFAULT_MODEL)
    ///     LEVEL_GEN_CHAMBERS  (default 5)
    ///     LEVEL_GEN_ENDPOINT  (default https://api.openai.com/v1)
    ///     LEVEL_GEN_PROMPT    (default the garrison prompt below)
    #[test]
    #[ignore = "A23: makes a real, paid request. Run with --ignored and OPENAI_API_KEY set."]
    fn a23_live_generation_against_a_real_model() {
        use crate::ai_client::{HttpModelClient, ModelSettings};

        let env = |key: &str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
        assert!(
            crate::ai_client::api_key_from_env().is_some(),
            "OPENAI_API_KEY is not set, so there is nothing to run this against. \
             This test must never be reported as passed when it has not run."
        );

        let params = GenerationParams {
            prompt: env("LEVEL_GEN_PROMPT").unwrap_or_else(|| {
                "A garrison of connected stone halls, laid out side to side. Broad flat floors \
                 with room to fight, an orc or two per hall, death traps only at the edges. A \
                 short optional side branch off the second hall with a coin reward. The last \
                 hall is a large arena."
                    .to_string()
            }),
            extent: Vec2::new(6000.0, 3000.0),
            chambers: env("LEVEL_GEN_CHAMBERS")
                .and_then(|c| c.parse().ok())
                .unwrap_or(5),
        };
        let settings = ModelSettings {
            endpoint: env("LEVEL_GEN_ENDPOINT")
                .unwrap_or_else(|| crate::ai_client::DEFAULT_ENDPOINT.to_string()),
            // The model the editor actually ships with, so what a first-time user gets is
            // what this exercises. An earlier version defaulted to a different model, which
            // left the shipped default as the one thing no test of any kind could see.
            model: env("LEVEL_GEN_MODEL")
                .unwrap_or_else(|| crate::ai_client::DEFAULT_MODEL.to_string()),
            seed: Some(0),
        };
        // A pass has to mean a real service was reached. Pointed at a loopback stub, no key
        // is needed and nothing outside this machine is proven, so refuse rather than report
        // a pass that says nothing.
        let endpoint = crate::ai_client::parse_endpoint(&settings.endpoint)
            .expect("the endpoint must be usable");
        assert!(
            endpoint.needs_api_key(),
            "{} is on this machine, so a pass here would prove nothing about a real service",
            settings.endpoint
        );
        println!("\n== A23: {} at {} ==", settings.model, settings.endpoint);
        println!("prompt: {}\n", params.prompt);

        let mut client = HttpModelClient::new(settings);
        let cancel = Arc::new(AtomicBool::new(false));
        let started = std::time::Instant::now();
        let generated = match generate(
            &mut client,
            &params,
            &mut |stage| println!("  [{:>5.1}s] {stage:?}", started.elapsed().as_secs_f32()),
            &cancel,
        ) {
            Ok(g) => g,
            Err(e) => panic!("\nA23 FAILED after {:.1}s: {e}\n", started.elapsed().as_secs_f32()),
        };

        // Exactly what A18 asserts of a transcript, now of a real reply.
        //
        // The emptiness checks come first and are not a formality: the per-entity loop
        // asserts nothing about an empty list, and a level with no floor at all could in
        // principle be called reachable if the exit happened to lie where a three-second fall
        // ends. Vanishingly unlikely, but it should be excluded by an assertion rather than
        // by luck.
        assert!(
            !generated.level.entities.is_empty(),
            "a delivered level with no entities in it is not a level"
        );
        assert!(
            generated.route_moves > 0,
            "a route that takes no moves means the exit was underfoot, not that the area works"
        );
        for entity in &generated.level.entities {
            match entity {
                LevelEntity::Polygon { polygon_type, vertices, .. } => {
                    let kind = polygon_type.as_deref().unwrap_or("");
                    assert!(POLYGON_TYPES.contains(&kind), "unknown polygon type {kind:?}");
                    assert!(vertices.len() >= 3);
                }
                LevelEntity::Bitmap { bitmap_name, size, .. } => {
                    assert!(
                        BITMAP_NAMES.contains(&bitmap_name.as_str()),
                        "unknown sprite {bitmap_name:?}"
                    );
                    assert!(size[0] > 0.0 && size[1] > 0.0);
                }
            }
        }
        let spawn = generated.level.spawn.expect("a delivered level carries its spawn");
        let exit = generated.level.exit.expect("and its exit");
        assert_eq!(generated.level.level_size, Some([params.extent.x, params.extent.y]));

        // And the route really is walkable, re-proven here rather than taken on trust.
        let problem = problem_for(
            &generated.level,
            Pos2::new(spawn[0], spawn[1]),
            Pos2::new(exit[0], exit[1]),
            params.extent,
        );
        match traversal::prove(&problem) {
            Verdict::Reachable(route) => assert!(
                traversal::route_arrives(&problem, &route),
                "the delivered route did not replay to the exit"
            ),
            Verdict::Unreachable(f) => {
                panic!("the delivered level is not walkable: {}", traversal::explain(&f, problem.exit))
            }
        }

        println!(
            "\nA23 PASSED in {:.1}s\n  chambers: {:?}\n  entities: {}\n  route: {} moves, {:.1}s of play\n  {}",
            started.elapsed().as_secs_f32(),
            generated.chambers,
            generated.level.entities.len(),
            generated.route_moves,
            generated.route_secs,
            if generated.is_the_models_own() {
                "every entity is the model's own work".to_string()
            } else {
                format!("REPAIRED: {} ledge(s) added — {:?}", generated.repairs.len(), generated.repairs)
            }
        );
    }

    #[test]
    fn the_same_transcript_twice_produces_the_same_level() {
        let cancel = Arc::new(AtomicBool::new(false));
        let params = GenerationParams {
            prompt: "a flooded cistern".to_string(),
            extent: Vec2::new(4000.0, 2000.0),
            chambers: 2,
        };
        let run = || {
            let mut client = ScriptedClient::new(a_good_transcript());
            let g = generate(&mut client, &params, &mut |_| {}, &cancel).expect("generates");
            format!("{:?}", g.level.entities)
        };
        assert_eq!(run(), run(), "given the same replies, a run reproduces the same level");
    }

    #[test]
    fn a_model_that_never_returns_usable_json_fails_with_the_reason() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut client = ScriptedClient::always("I would rather not.");
        let err = generate(&mut client, &GenerationParams::default(), &mut |_| {}, &cancel)
            .expect_err("nothing usable came back");
        match &err {
            GenError::Invalid(why) => assert!(!why.is_empty(), "the reason is given"),
            other => panic!("expected Invalid, got {other:?}"),
        }
        assert!(
            err.to_string().contains("has not been touched"),
            "the user is told their level is safe: {err}"
        );
        assert_eq!(
            client.requests.len(),
            MAX_CHAMBER_RETRIES + 1,
            "the outline is asked for, then retried, and then it gives up"
        );
    }
}
