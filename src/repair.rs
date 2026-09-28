//! Making a level the model wrote into one that can actually be walked.
//!
//! Repair is the last resort, after the model has been told what is wrong and asked again.
//! It is deliberately dull: find where the route dies, put one small ledge within the
//! character's reach in the direction of the exit, and prove it again. Repeat until the
//! route closes or the budget runs out. Nothing is moved and nothing is deleted — the
//! model's own geometry is left exactly as written, with stepping stones added beside it —
//! so "repaired" always means "the model's level, plus these ledges", and the report can
//! name every one of them.

use egui::{Pos2, Vec2};

use crate::game_config as cfg;
use crate::level_data::{LevelData, LevelEntity};
use crate::level_gen::GenError;
use crate::sim::{CollisionPoly, Rope, World};
use crate::traversal::{self, Problem, Verdict};

/// How many ledges may be added before the attempt is abandoned. A level needing more than
/// this is not a level with a gap in it; it is one the model never really built.
pub const MAX_EDITS: usize = 12;

/// The ledges repair adds. Wide enough to land on, thin enough not to reshape the chamber.
pub const LEDGE_WIDTH: f32 = 260.0;
pub const LEDGE_THICKNESS: f32 = 40.0;

/// How far one stepping stone may be from the last foothold. Both are kept well inside what
/// the character can do, so a stone is always reachable from the ledge it is placed beyond.
pub const STEP_X: f32 = 300.0; // against DASH_DISTANCE 450
pub const STEP_UP: f32 = 200.0; // against JUMP_HEIGHT 365

/// A repaired level, and what was done to it.
#[derive(Clone, Debug)]
pub struct Repaired {
    pub level: LevelData,
    /// One line per edit, in the order they were made.
    pub notes: Vec<String>,
    pub route_moves: usize,
    /// How long the proven route takes to run, in simulated seconds.
    pub route_secs: f32,
}

/// The collision world of a level, built exactly as play mode builds it: every polygon is
/// solid, a `wall_tool` polygon is additionally climbable, and `rope_tool` entries are
/// ropes. If this ever disagreed with play mode, a proof would not mean anything.
pub fn world_of(level: &LevelData) -> World {
    let mut polys = Vec::new();
    let mut ropes = Vec::new();
    for entity in &level.entities {
        match entity {
            LevelEntity::Polygon { vertices, polygon_type, .. } => {
                let points: Vec<Pos2> = vertices.iter().map(|v| Pos2::new(v[0], v[1])).collect();
                let climbable =
                    polygon_type.as_deref() == Some(cfg::CLIMBABLE_POLYGON_TYPE);
                if let Some(poly) = CollisionPoly::new(&points, climbable) {
                    polys.push(poly);
                }
            }
            LevelEntity::Bitmap { position, bitmap_name, size } if bitmap_name == cfg::ROPE_BITMAP => {
                ropes.push(Rope {
                    anchor: Pos2::new(position[0], position[1]),
                    length: size[1],
                });
            }
            LevelEntity::Bitmap { .. } => {}
        }
    }
    World { polys, ropes }
}

/// A solid ledge whose top surface is at `top`, centred on `centre_x`.
fn ledge(centre_x: f32, top: f32) -> LevelEntity {
    let half = LEDGE_WIDTH / 2.0;
    LevelEntity::Polygon {
        vertices: vec![
            [centre_x - half, top],
            [centre_x + half, top],
            [centre_x + half, top + LEDGE_THICKNESS],
            [centre_x - half, top + LEDGE_THICKNESS],
        ],
        // Climbable, so a stone placed against a rise can be worked with from below too.
        polygon_type: Some(cfg::CLIMBABLE_POLYGON_TYPE.to_string()),
        color: None,
    }
}

/// Where to put the next stepping stone: one reachable step from `from` toward `exit`.
fn next_stone(from: Pos2, exit: Pos2, extent: Vec2) -> Pos2 {
    let to_exit = exit - from;
    let dx = to_exit.x.clamp(-STEP_X, STEP_X);
    // Y is down, so a negative dy is upward. Rising is limited by the jump; falling is free,
    // but a stone is only useful if the character can get back off it, so it is capped too.
    let dy = to_exit.y.clamp(-STEP_UP, STEP_UP);
    let x = (from.x + dx).clamp(LEDGE_WIDTH, extent.x - LEDGE_WIDTH);
    // The stone's *surface* must sit below the character's feet at the target height.
    let y = (from.y + dy + cfg::PLAYER_CAPSULE_HALF_EXTENT_Y)
        .clamp(LEDGE_THICKNESS, extent.y - LEDGE_THICKNESS);
    Pos2::new(x, y)
}

/// Add stepping stones until the route closes, or give up.
pub fn repair(level: &LevelData, problem: &Problem) -> Result<Repaired, GenError> {
    let extent = problem.level_size.unwrap_or(Vec2::new(6000.0, 3000.0));
    let mut level = level.clone();
    let mut notes = Vec::new();
    let mut placed: Vec<Pos2> = Vec::new();
    let mut last_reason = String::new();

    for _ in 0..MAX_EDITS {
        let attempt = Problem {
            world: world_of(&level),
            spawn: problem.spawn,
            exit: problem.exit,
            level_size: problem.level_size,
        };
        match traversal::prove(&attempt) {
            // The same belt-and-braces check the un-repaired path makes: a route that does
            // not survive being replayed is not a proof. The repair branch used to skip it,
            // and a real divergence has since been found and fixed in the prover, so the
            // check is not theoretical.
            Verdict::Reachable(route) if traversal::route_arrives(&attempt, &route) => {
                return Ok(Repaired {
                    level,
                    notes,
                    route_moves: route.moves.len(),
                    route_secs: route.duration(),
                })
            }
            Verdict::Reachable(_) => {
                return Err(GenError::Unprovable(
                    "The repaired route did not survive being replayed, so it is not a proof."
                        .to_string(),
                ))
            }
            Verdict::Unreachable(frontier) => {
                last_reason = traversal::explain(&frontier, problem.exit);
                let stone = next_stone(frontier.deepest, problem.exit, extent);
                // A stone in the same place twice means the last one did not help and the
                // frontier has not moved; adding it again would only spin. This compares
                // the actual positions: an earlier version matched the formatted note text,
                // which also holds the frontier each stone was placed beyond, so a
                // legitimate stone could collide with an unrelated coordinate and abandon
                // repair with its budget untouched.
                if placed.iter().any(|p| (*p - stone).length() < 1.0) {
                    break;
                }
                placed.push(stone);
                level.entities.push(ledge(stone.x, stone.y));
                notes.push(format!(
                    "added a {LEDGE_WIDTH:.0} x {LEDGE_THICKNESS:.0} ledge at \
                     ({:.0}, {:.0}) to carry the route on from the ledge at ({:.0}, {:.0})",
                    stone.x, stone.y, frontier.deepest.x, frontier.deepest.y
                ));
            }
        }
    }

    // One last look, in case the final edit was the one that closed it.
    let attempt = Problem {
        world: world_of(&level),
        spawn: problem.spawn,
        exit: problem.exit,
        level_size: problem.level_size,
    };
    if let Verdict::Reachable(route) = traversal::prove(&attempt) {
        if traversal::route_arrives(&attempt, &route) {
            return Ok(Repaired {
                level,
                notes,
                route_moves: route.moves.len(),
                route_secs: route.duration(),
            });
        }
    }

    Err(GenError::Unprovable(format!(
        "After {} attempted repair(s), the route still does not reach the exit. {last_reason}",
        notes.len()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_entity(x0: f32, y0: f32, x1: f32, y1: f32, kind: &str) -> LevelEntity {
        LevelEntity::Polygon {
            vertices: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            polygon_type: Some(kind.to_string()),
            color: None,
        }
    }

    /// A level whose two ledges are too far apart to cross. A running jump with an air dash
    /// carries the character a surprising distance — around 900 px — so the gap here is
    /// 1600 px, comfortably past anything the movement kit can do in one go.
    fn broken_level() -> (LevelData, Problem) {
        let extent = Vec2::new(4000.0, 2000.0);
        let spawn = Pos2::new(300.0, 1100.0);
        let exit = Pos2::new(2600.0, 1155.0);
        let level = LevelData {
            level_size: Some([extent.x, extent.y]),
            spawn: Some([spawn.x, spawn.y]),
            exit: Some([exit.x, exit.y]),
            entities: vec![
                rect_entity(0.0, 1200.0, 600.0, 1400.0, "blocker_tool"),
                rect_entity(2200.0, 1200.0, 3000.0, 1400.0, "blocker_tool"),
            ],
            ..LevelData::default()
        };
        let problem = Problem {
            world: world_of(&level),
            spawn,
            exit,
            level_size: Some(extent),
        };
        (level, problem)
    }

    #[test]
    fn the_collision_world_matches_how_play_mode_reads_a_level() {
        let level = LevelData {
            entities: vec![
                rect_entity(0.0, 0.0, 100.0, 100.0, "wall_tool"),
                rect_entity(200.0, 0.0, 300.0, 100.0, "blocker_tool"),
                LevelEntity::Bitmap {
                    position: [400.0, 50.0],
                    bitmap_name: "rope_tool".into(),
                    size: [6.0, 300.0],
                },
                LevelEntity::Bitmap {
                    position: [500.0, 50.0],
                    bitmap_name: "coin_tool".into(),
                    size: [64.0, 64.0],
                },
            ],
            ..LevelData::default()
        };
        let world = world_of(&level);
        assert_eq!(world.polys.len(), 2, "both polygons are solid");
        assert!(world.polys[0].climbable, "wall_tool is climbable");
        assert!(!world.polys[1].climbable, "blocker_tool is not");
        assert_eq!(world.ropes.len(), 1, "the rope entry became a rope");
        assert_eq!(world.ropes[0].anchor, Pos2::new(400.0, 50.0));
        assert_eq!(world.ropes[0].length, 300.0);
    }

    #[test]
    fn a_degenerate_polygon_is_dropped_rather_than_breaking_the_world() {
        let level = LevelData {
            entities: vec![LevelEntity::Polygon {
                vertices: vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], // collinear: no hull
                polygon_type: Some("blocker_tool".into()),
                color: None,
            }],
            ..LevelData::default()
        };
        assert!(world_of(&level).polys.is_empty());
    }

    // A11 — repair closes a route the model left open, and the same prover confirms it.
    #[test]
    fn repair_bridges_an_uncrossable_gap_and_the_route_is_then_proven() {
        let (level, problem) = broken_level();
        assert!(
            !traversal::prove(&problem).is_reachable(),
            "the fixture must start out genuinely unprovable"
        );

        let repaired = repair(&level, &problem).expect("a 900 px gap is bridgeable with stones");
        assert!(!repaired.notes.is_empty(), "repair must say what it did");
        assert!(
            repaired.notes[0].contains("added a"),
            "the note names the edit: {}",
            repaired.notes[0]
        );

        // The delivered level is checked by the same prover that rejected the original.
        let after = Problem {
            world: world_of(&repaired.level),
            spawn: problem.spawn,
            exit: problem.exit,
            level_size: problem.level_size,
        };
        match traversal::prove(&after) {
            Verdict::Reachable(route) => {
                let landed = traversal::replay(&after, &route);
                assert!(
                    (landed - problem.exit).length() <= traversal::ARRIVAL_RADIUS,
                    "the repaired route must replay to the exit"
                );
            }
            Verdict::Unreachable(f) => {
                panic!("repair returned a level that does not work: {}", traversal::explain(&f, problem.exit))
            }
        }
    }

    #[test]
    fn repair_leaves_every_entity_the_model_wrote_exactly_where_it_was() {
        let (level, problem) = broken_level();
        let before = level.entities.len();
        let repaired = repair(&level, &problem).expect("bridgeable");
        assert!(repaired.level.entities.len() > before, "stones were added");
        for (i, original) in level.entities.iter().enumerate() {
            let kept = &repaired.level.entities[i];
            assert_eq!(
                format!("{original:?}"),
                format!("{kept:?}"),
                "entity {i} the model wrote must be untouched"
            );
        }
    }

    // A12 — when repair cannot succeed, it fails and says so.
    #[test]
    fn repair_that_cannot_reach_the_exit_fails_and_says_so() {
        let extent = Vec2::new(2600.0, 1600.0);
        // The exit is sealed inside a solid box. No ledge placed outside it can ever put
        // the character in there, so repair runs out of useful edits and must give up.
        let level = LevelData {
            level_size: Some([extent.x, extent.y]),
            entities: vec![
                rect_entity(0.0, 1000.0, 600.0, 1200.0, "blocker_tool"),
                // The box around the exit: floor, ceiling and both walls.
                rect_entity(1800.0, 1000.0, 2400.0, 1100.0, "blocker_tool"),
                rect_entity(1800.0, 600.0, 2400.0, 700.0, "blocker_tool"),
                rect_entity(1800.0, 700.0, 1900.0, 1000.0, "blocker_tool"),
                rect_entity(2300.0, 700.0, 2400.0, 1000.0, "blocker_tool"),
            ],
            ..LevelData::default()
        };
        let problem = Problem {
            world: world_of(&level),
            spawn: Pos2::new(300.0, 900.0),
            exit: Pos2::new(2100.0, 955.0),
            level_size: Some(extent),
        };
        let err = repair(&level, &problem).expect_err("a sealed exit cannot be reached");
        match &err {
            GenError::Unprovable(why) => {
                assert!(why.contains("repair"), "the reason mentions repair: {why}");
                assert!(why.contains("does not reach the exit"), "{why}");
            }
            other => panic!("expected Unprovable, got {other:?}"),
        }
        assert!(
            err.to_string().contains("has not been touched"),
            "the user is told their level is safe: {err}"
        );
    }

    #[test]
    fn a_stepping_stone_is_always_within_the_characters_reach() {
        let extent = Vec2::new(4000.0, 2000.0);
        let from = Pos2::new(500.0, 1000.0);
        for exit in [
            Pos2::new(3000.0, 1000.0),
            Pos2::new(500.0, 200.0),
            Pos2::new(100.0, 1900.0),
            Pos2::new(3900.0, 100.0),
        ] {
            let stone = next_stone(from, exit, extent);
            assert!(
                (stone.x - from.x).abs() <= STEP_X + 0.001,
                "a stone must be within a dash of the ledge it follows"
            );
            assert!(
                stone.x >= 0.0 && stone.x <= extent.x && stone.y >= 0.0 && stone.y <= extent.y,
                "and inside the level"
            );
        }
    }

    #[test]
    fn a_stone_is_refused_only_when_that_stone_was_already_placed() {
        // The guard must compare stone positions, not the text of the notes: every note
        // also names the frontier the stone was placed beyond, so matching on text could
        // refuse a perfectly good stone whose coordinates happened to equal some earlier
        // frontier's. This gap is wide enough to need several stones in a row, which is
        // what exercises the guard at all.
        let extent = Vec2::new(5000.0, 2000.0);
        let spawn = Pos2::new(300.0, 1100.0);
        let exit = Pos2::new(3600.0, 1155.0);
        let level = LevelData {
            level_size: Some([extent.x, extent.y]),
            entities: vec![
                rect_entity(0.0, 1200.0, 600.0, 1400.0, "blocker_tool"),
                rect_entity(3200.0, 1200.0, 4000.0, 1400.0, "blocker_tool"),
            ],
            ..LevelData::default()
        };
        let problem = Problem { world: world_of(&level), spawn, exit, level_size: Some(extent) };
        let repaired = repair(&level, &problem).expect("bridgeable with several stones");
        assert!(
            repaired.notes.len() >= 2,
            "this gap needs more than one stone, so the guard is genuinely exercised: {:?}",
            repaired.notes
        );
        // Every stone that was placed is at a distinct position.
        let added: Vec<&LevelEntity> = repaired.level.entities[level.entities.len()..].iter().collect();
        assert_eq!(added.len(), repaired.notes.len(), "one entity per note");
        for (i, a) in added.iter().enumerate() {
            for b in added.iter().skip(i + 1) {
                assert_ne!(format!("{a:?}"), format!("{b:?}"), "no stone is placed twice");
            }
        }
    }

    #[test]
    fn a_level_that_already_works_is_returned_untouched() {
        let extent = Vec2::new(2000.0, 1600.0);
        let level = LevelData {
            level_size: Some([extent.x, extent.y]),
            entities: vec![rect_entity(0.0, 1200.0, 2000.0, 1400.0, "blocker_tool")],
            ..LevelData::default()
        };
        let problem = Problem {
            world: world_of(&level),
            spawn: Pos2::new(200.0, 1100.0),
            exit: Pos2::new(240.0, 1155.0),
            level_size: Some(extent),
        };
        let repaired = repair(&level, &problem).expect("already fine");
        assert!(repaired.notes.is_empty(), "nothing needed doing, so nothing was done");
        assert_eq!(repaired.level.entities.len(), level.entities.len());
    }
}
