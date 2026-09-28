//! Proving that a level can actually be played.
//!
//! A generated level is delivered only once the editor has proven a route through it — not
//! estimated one from the movement constants, but walked one. The prover drives the same
//! [`crate::sim::Simulation`] play mode drives, on the same fixed step, against the same
//! collision world, so "the prover says it is playable" and "you can play it" are the same
//! statement. Nothing here changes how the simulation behaves; it only presses buttons.
//!
//! The search is a breadth-first expansion over *resting states* — standing on ground,
//! clinging to a wall — connected by whole moves: run off a ledge, jump at one of several
//! hold lengths, dash, wall jump, climb. Each move is simulated frame by frame and the
//! state it actually ends in is what gets recorded, so a move is reachable exactly when the
//! simulation says it is. Reached states are rounded onto a grid so that the search
//! terminates rather than chasing pixel-level variations of the same foothold.

use std::collections::{HashMap, VecDeque};

use egui::{Pos2, Vec2};

use crate::game_config as cfg;
use crate::sim::{Input, Simulation, State, World, FIXED_DT};

/// How finely two positions must differ to count as different footholds. Coarse enough that
/// the search closes quickly, fine enough that a ledge and the gap beside it are distinct.
pub const GRID: f32 = 40.0;

/// A move is abandoned after this long. The longest useful move is a full jump across a
/// wide gap; anything still airborne well past that is falling to its death.
pub const MAX_MOVE_SECS: f32 = 4.0;

/// Stop expanding after this many states. A generated area is a few hundred footholds; this
/// is a guard against pathological geometry, not a normal limit.
pub const MAX_STATES: usize = 60_000;

/// How close to the exit counts as arriving.
pub const ARRIVAL_RADIUS: f32 = 60.0;

/// One button pattern held for a number of frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub input: Input,
    pub frames: u32,
}

/// A move the character can attempt from a resting state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Primitive {
    pub name: &'static str,
    /// Held in order; the move ends when these run out or the character comes to rest.
    pub segments: Vec<Segment>,
}

fn frames(secs: f32) -> u32 {
    (secs / FIXED_DT).round().max(1.0) as u32
}

fn held(left: bool, right: bool, up: bool, down: bool, jump: bool, dash: bool) -> Input {
    Input { left, right, up, down, jump, dash }
}

/// The moves tried from every resting state.
///
/// This is deliberately a fixed repertoire rather than a search over arbitrary input: it
/// covers what the character can actually do — run, jump at four hold lengths, dash on the
/// ground and in the air, wall jump, climb — in both directions, which is enough to decide
/// whether a level's geometry is passable. A route the repertoire cannot find is a route a
/// player would have to be very inventive to walk.
pub fn primitives_for(world: &World) -> Vec<Primitive> {
    let has_ropes = !world.ropes.is_empty();
    let mut out = Vec::new();

    for &(left, right) in &[(true, false), (false, true)] {
        let toward = |secs: f32| Segment {
            input: held(left, right, false, false, false, false),
            frames: frames(secs),
        };
        let away = |secs: f32| Segment {
            input: held(right, left, false, false, false, false),
            frames: frames(secs),
        };
        let dir = |l: bool, r: bool| if l { "left" } else if r { "right" } else { "" };
        let name: &'static str = if left { "left" } else { "right" };
        let _ = dir;

        // Walk along, and walk off the end of a ledge.
        for &secs in &[0.25_f32, 0.6, 1.2] {
            out.push(Primitive {
                name: if left { "run left" } else { "run right" },
                segments: vec![toward(secs)],
            });
        }

        // Jump, held for four different lengths, carrying the run.
        for &hold in &[0.08_f32, 0.16, 0.30, 0.60] {
            out.push(Primitive {
                name: if left { "jump left" } else { "jump right" },
                segments: vec![
                    toward(0.18),
                    Segment { input: held(left, right, false, false, true, false), frames: frames(hold) },
                    toward(1.2),
                ],
            });
        }

        // A standing jump, for a shaft with no room for a run-up.
        out.push(Primitive {
            name: if left { "standing jump left" } else { "standing jump right" },
            segments: vec![
                Segment { input: held(false, false, false, false, true, false), frames: frames(0.30) },
                toward(1.0),
            ],
        });

        // Dash along the ground, and dash out of a jump at two different moments: the dash
        // is what clears a gap wider than a jump can carry.
        out.push(Primitive {
            name: if left { "dash left" } else { "dash right" },
            segments: vec![
                Segment { input: held(left, right, false, false, false, true), frames: frames(0.05) },
                toward(1.2),
            ],
        });
        for &delay in &[0.10_f32, 0.24] {
            out.push(Primitive {
                name: if left { "jump-dash left" } else { "jump-dash right" },
                segments: vec![
                    toward(0.18),
                    Segment { input: held(left, right, false, false, true, false), frames: frames(0.30) },
                    toward(delay),
                    Segment { input: held(left, right, false, false, false, true), frames: frames(0.05) },
                    toward(1.2),
                ],
            });
        }

        // Cling to the wall on this side and jump off it, the move a shaft is climbed
        // with. The jump launches away from the wall on its own and ignores input for
        // `WALL_JUMP_INPUT_LOCK`, so steering away afterwards carries it across.
        out.push(Primitive {
            name: if left { "wall jump off left wall" } else { "wall jump off right wall" },
            segments: vec![
                toward(0.12),
                Segment { input: held(left, right, false, false, true, false), frames: frames(0.24) },
                away(1.0),
            ],
        });

        // Press into a `wall_tool` face and climb it, up or down.
        for &(vname, up, down) in
            &[("climb up", true, false), ("climb down", false, true)]
        {
            for &secs in &[0.5_f32, 1.5] {
                out.push(Primitive {
                    name: vname,
                    segments: vec![Segment {
                        input: held(left, right, up, down, false, false),
                        frames: frames(secs),
                    }],
                });
            }
        }

        // Reach a rope, ride it, and let go at the far side of the swing. Only offered
        // where there is a rope to ride: the same buttons in a level with none are just a
        // jump with `up` held, and reporting that as a rope swing would be a lie.
        let rides: &[f32] = if has_ropes { &[0.45, 0.9, 1.4] } else { &[] };
        for &ride in rides {
            out.push(Primitive {
                name: "rope swing",
                segments: vec![
                    toward(0.25),
                    Segment { input: held(left, right, false, false, true, false), frames: frames(0.24) },
                    Segment { input: held(left, right, true, false, false, false), frames: frames(ride) },
                    Segment { input: held(left, right, false, false, true, false), frames: frames(0.20) },
                    toward(1.0),
                ],
            });
        }

        let _ = name;
    }

    out
}

/// A position rounded onto the search grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Cell(i32, i32);

fn cell(p: Pos2) -> Cell {
    Cell((p.x / GRID).floor() as i32, (p.y / GRID).floor() as i32)
}

/// What the prover concluded.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// The exit was reached, by these moves.
    Reachable(Route),
    /// It was not, and this is where the route died.
    Unreachable(Frontier),
}

impl Verdict {
    /// Used by the tests that hold the prover honest; the pipeline itself matches on the
    /// verdict because it needs the route or the frontier out of it, not just the answer.
    #[allow(dead_code)]
    pub fn is_reachable(&self) -> bool {
        matches!(self, Verdict::Reachable(_))
    }
}

/// A proven way through: the buttons that get there, and the moves they amount to.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    /// Every segment in order, from the spawn. Replaying these through a fresh
    /// `Simulation::new(spawn)` reaches the exit — including the frames spent falling to the
    /// first foothold, which are the route's first segment. Nothing is left implicit: a
    /// caller that replays these and nothing else gets the trajectory the search found.
    pub segments: Vec<Segment>,
    /// The names of the moves used, for the report.
    pub moves: Vec<&'static str>,
    /// Where the character finished.
    pub arrived_at: Pos2,
}

impl Route {
    /// Total simulated time the route takes.
    pub fn duration(&self) -> f32 {
        self.segments.iter().map(|s| s.frames as f32 * FIXED_DT).sum()
    }
}

/// Where a search that failed got stuck.
#[derive(Clone, Debug, PartialEq)]
pub struct Frontier {
    /// The reachable resting place closest to the exit: the ledge the route dies on.
    pub deepest: Pos2,
    /// How far that still is from the exit.
    pub distance_to_exit: f32,
    /// How many distinct footholds were reachable at all.
    pub reachable_footholds: usize,
    /// Every reachable resting place, so repair knows what it has to work with.
    pub reachable: Vec<Pos2>,
    /// The closest place the character could stand that it cannot get to: the nearest
    /// unreached foothold beyond the ledge the route dies on, and how far that is. `None`
    /// when the level offers nothing else to stand on at all.
    pub nearest_unreached: Option<(Pos2, f32)>,
}

/// One reached resting state.
#[derive(Clone, Debug)]
struct Node {
    sim: Simulation,
    /// The buttons that reached it from the spawn.
    segments: Vec<Segment>,
    moves: Vec<&'static str>,
}

/// Somewhere a next move could start from: standing on something, or clinging to a wall.
/// A character still in mid-air has not arrived anywhere yet.
fn resting(sim: &Simulation) -> bool {
    sim.grounded || matches!(sim.state, State::WallSliding | State::Climbing)
}

/// Whether this state is still inside the level and usable at all.
fn inside(sim: &Simulation, bounds: Option<(Vec2, f32)>) -> bool {
    if !sim.pos.x.is_finite() || !sim.pos.y.is_finite() {
        return false;
    }
    match bounds {
        // Fallen out of the level, or run off its side: not a foothold.
        Some((size, slack)) => {
            sim.pos.y <= size.y + slack
                && sim.pos.x >= -slack
                && sim.pos.x <= size.x + slack
                && sim.pos.y >= -slack
        }
        None => true,
    }
}

/// Run `segments` on a copy of `sim` and return where it comes to rest, together with the
/// number of further no-input frames that were needed to get there. `None` if it never comes
/// to rest — still falling when the move's time runs out, or gone from the world.
///
/// A move that ends with the character already resting is taken at once: that is what lets
/// a climb stop part-way up a face, and a wall jump finish clinging to the opposite wall,
/// instead of every move sliding back down to the floor before it counts.
///
/// The settle count is returned rather than swallowed because it is part of what the player
/// did. An earlier version dropped it, so a recorded route was missing those frames and
/// replaying it landed somewhere else — which is precisely what a proof must not do.
fn apply(
    sim: &Simulation,
    world: &World,
    segments: &[Segment],
    bounds: Option<(Vec2, f32)>,
) -> Option<(Simulation, u32)> {
    let mut next = sim.clone();
    let mut elapsed = 0.0_f32;
    for segment in segments {
        for _ in 0..segment.frames {
            next.step(world, segment.input, FIXED_DT);
            elapsed += FIXED_DT;
            if elapsed > MAX_MOVE_SECS || !inside(&next, bounds) {
                return None;
            }
        }
    }
    if resting(&next) {
        return Some((next, 0));
    }
    // Still in the air: let it fall where it is going, with no further input.
    let mut settle = 0u32;
    while elapsed <= MAX_MOVE_SECS {
        next.step(world, Input::default(), FIXED_DT);
        elapsed += FIXED_DT;
        settle += 1;
        if !inside(&next, bounds) {
            return None;
        }
        if resting(&next) {
            return Some((next, settle));
        }
    }
    None
}

/// What the prover is asked to prove.
#[derive(Clone, Debug)]
pub struct Problem {
    pub world: World,
    pub spawn: Pos2,
    pub exit: Pos2,
    /// The level's extent, used to notice a character that has left it.
    pub level_size: Option<Vec2>,
}

/// Search for a route from the spawn to the exit.
pub fn prove(problem: &Problem) -> Verdict {
    prove_with(problem, &primitives_for(&problem.world))
}

/// Search using only `moves`. Proving with a repertoire that is missing one ability is how
/// a test shows that ability is what makes a route possible: the same geometry, proven with
/// the dash and unprovable without it, is a gap that genuinely needs a dash.
pub fn prove_with(problem: &Problem, moves: &[Primitive]) -> Verdict {
    let bounds = problem.level_size.map(|s| (s, 400.0_f32));

    // Let the character fall to whatever it is standing on before the search begins: a
    // spawn is a point in the air, and the first foothold is wherever it lands.
    let mut start = Simulation::new(problem.spawn);
    let mut settled = false;
    let mut start_frames = 0u32;
    for _ in 0..frames(3.0) {
        start.step(&problem.world, Input::default(), FIXED_DT);
        start_frames += 1;
        if resting(&start) {
            settled = true;
            break;
        }
    }
    // The fall to the first foothold is part of the route, so it is its first segment rather
    // than something `replay` has to know to repeat.
    let opening: Vec<Segment> = (start_frames > 0)
        .then(|| vec![Segment { input: Input::default(), frames: start_frames }])
        .unwrap_or_default();

    let arrived = |p: Pos2| (p - problem.exit).length() <= ARRIVAL_RADIUS;

    if !settled {
        // Nothing under the spawn at all. That is still a legitimate answer if the exit is
        // where the character fell to, but otherwise the route dies at the spawn.
        if arrived(start.pos) {
            return Verdict::Reachable(Route {
                segments: opening,
                moves: vec![],
                arrived_at: start.pos,
            });
        }
        return Verdict::Unreachable(Frontier {
            deepest: problem.spawn,
            distance_to_exit: (problem.spawn - problem.exit).length(),
            reachable_footholds: 0,
            reachable: vec![],
            nearest_unreached: nearest_unreached(&problem.world, &[], problem.spawn),
        });
    }

    if arrived(start.pos) {
        return Verdict::Reachable(Route { segments: opening, moves: vec![], arrived_at: start.pos });
    }

    let mut seen: HashMap<Cell, ()> = HashMap::new();
    seen.insert(cell(start.pos), ());
    let mut reachable = vec![start.pos];
    let mut best = (start.pos, (start.pos - problem.exit).length());

    let mut queue: VecDeque<Node> = VecDeque::new();
    queue.push_back(Node { sim: start, segments: opening, moves: vec![] });

    let mut expanded = 0usize;
    while let Some(node) = queue.pop_front() {
        expanded += 1;
        if expanded > MAX_STATES {
            break;
        }
        for primitive in moves {
            let Some((landed, settle)) =
                apply(&node.sim, &problem.world, &primitive.segments, bounds)
            else {
                continue;
            };
            let key = cell(landed.pos);
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, ());
            reachable.push(landed.pos);

            let mut segments = node.segments.clone();
            segments.extend_from_slice(&primitive.segments);
            // The frames spent coming to rest are part of the route: leave them out and
            // replaying it does something else.
            if settle > 0 {
                segments.push(Segment { input: Input::default(), frames: settle });
            }
            let mut names = node.moves.clone();
            names.push(primitive.name);

            let distance = (landed.pos - problem.exit).length();
            if distance < best.1 {
                best = (landed.pos, distance);
            }
            if distance <= ARRIVAL_RADIUS {
                return Verdict::Reachable(Route { segments, moves: names, arrived_at: landed.pos });
            }
            queue.push_back(Node { sim: landed, segments, moves: names });
        }
    }

    let nearest = nearest_unreached(&problem.world, &reachable, best.0);
    Verdict::Unreachable(Frontier {
        deepest: best.0,
        distance_to_exit: best.1,
        reachable_footholds: reachable.len(),
        reachable,
        nearest_unreached: nearest,
    })
}

/// Replay a route through a fresh simulation and report where it ends up. Used to check
/// that a proof really is one, and by the tests that hold the prover honest.
///
/// This presses the route's buttons and nothing else — it does not repeat the settle the
/// search began with, because that settle is the route's first segment. Anything the search
/// did that the route does not record would show up here as a divergence, which is the point.
pub fn replay(problem: &Problem, route: &Route) -> Pos2 {
    let mut sim = Simulation::new(problem.spawn);
    for segment in &route.segments {
        for _ in 0..segment.frames {
            sim.step(&problem.world, segment.input, FIXED_DT);
        }
    }
    sim.pos
}

/// Every place the level offers to stand on: the upper edge of each collision polygon,
/// sampled at its vertices and at their midpoints, lifted to where the character's centre
/// would be. These are candidate footholds, not reachable ones.
fn candidate_footholds(world: &World) -> Vec<Pos2> {
    let mut out = Vec::new();
    for poly in &world.polys {
        let top = poly.points.iter().fold(f32::INFINITY, |a, p| a.min(p.y));
        // The vertices along this polygon's upper edge, and the midpoints between them, so
        // a long ledge offers a foothold in its middle and not only at its corners.
        let mut upper: Vec<Pos2> = poly
            .points
            .iter()
            .copied()
            .filter(|p| (p.y - top).abs() < 1.0)
            .collect();
        upper.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut points = upper.clone();
        for pair in upper.windows(2) {
            points.push(Pos2::new((pair[0].x + pair[1].x) / 2.0, pair[0].y));
        }
        for p in points {
            out.push(Pos2::new(p.x, p.y - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y));
        }
    }
    out
}

/// The nearest candidate foothold that the search never reached, measured from `from`.
fn nearest_unreached(world: &World, reached: &[Pos2], from: Pos2) -> Option<(Pos2, f32)> {
    candidate_footholds(world)
        .into_iter()
        .filter(|c| reached.iter().all(|r| (*c - *r).length() > GRID))
        .map(|c| (c, (c - from).length()))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// Whether replaying a route really does reach the exit. The prover and the replay drive
/// the same simulation, so this always agrees with the search — which is exactly why it is
/// worth asserting before a level is handed over: if the two ever disagreed, the proof
/// would mean nothing.
pub fn route_arrives(problem: &Problem, route: &Route) -> bool {
    (replay(problem, route) - problem.exit).length() <= ARRIVAL_RADIUS
}

/// A sentence describing why a level could not be proven, in terms the model can act on.
pub fn explain(frontier: &Frontier, exit: Pos2) -> String {
    let nearest = match frontier.nearest_unreached {
        Some((at, distance)) => format!(
            " The nearest foothold beyond it is {distance:.0} px away, at ({:.0}, {:.0}) — \
             {:.0} px across and {:.0} px {}.",
            at.x,
            at.y,
            (at.x - frontier.deepest.x).abs(),
            (at.y - frontier.deepest.y).abs(),
            if at.y < frontier.deepest.y { "up" } else { "down" },
        ),
        None => " There is no other foothold anywhere in the level to reach.".to_string(),
    };
    format!(
        "The route dies on the ledge at ({:.0}, {:.0}). From there nothing further is \
         reachable, and the exit at ({:.0}, {:.0}) is still {:.0} px away.{nearest} Only {} \
         foothold(s) are reachable from the start at all. The character can clear a {:.0} px \
         jump, a {:.0} px dash (one in the air), and a wall jump of {:.0} px up by {:.0} px \
         across; it can climb a wall_tool face but nothing else.",
        frontier.deepest.x,
        frontier.deepest.y,
        exit.x,
        exit.y,
        frontier.distance_to_exit,
        frontier.reachable_footholds,
        cfg::JUMP_HEIGHT,
        cfg::DASH_DISTANCE,
        cfg::WALL_JUMP_HEIGHT,
        cfg::WALL_JUMP_HORIZONTAL_DISTANCE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::CollisionPoly;

    /// An axis-aligned solid rectangle.
    fn rect(x0: f32, y0: f32, x1: f32, y1: f32, climbable: bool) -> CollisionPoly {
        CollisionPoly::new(
            &[
                Pos2::new(x0, y0),
                Pos2::new(x1, y0),
                Pos2::new(x1, y1),
                Pos2::new(x0, y1),
            ],
            climbable,
        )
        .expect("a rectangle is a polygon")
    }

    /// Two ledges at the same height with `gap` of nothing between them.
    fn two_ledges(gap: f32) -> Problem {
        let left_end = 600.0;
        let right_start = left_end + gap;
        Problem {
            world: World {
                polys: vec![
                    rect(0.0, 800.0, left_end, 1000.0, false),
                    rect(right_start, 800.0, right_start + 600.0, 1000.0, false),
                ],
                ropes: vec![],
            },
            spawn: Pos2::new(300.0, 700.0),
            exit: Pos2::new(right_start + 300.0, 760.0),
            level_size: Some(Vec2::new(right_start + 800.0, 1400.0)),
        }
    }

    // A7 — a crossable gap is proven, and the proof replays.
    #[test]
    fn a_two_hundred_pixel_gap_is_proven_and_the_route_replays() {
        let problem = two_ledges(200.0);
        let verdict = prove(&problem);
        let route = match verdict {
            Verdict::Reachable(route) => route,
            Verdict::Unreachable(f) => panic!("a 200 px gap must be crossable: {}", explain(&f, problem.exit)),
        };
        assert!(!route.segments.is_empty(), "crossing takes at least one move");

        let landed = replay(&problem, &route);
        assert!(
            (landed - problem.exit).length() <= ARRIVAL_RADIUS,
            "replaying the proof must reach the exit, but finished at ({:.0}, {:.0}) \
             which is {:.0} px away",
            landed.x,
            landed.y,
            (landed - problem.exit).length(),
        );
    }

    // A8 — an uncrossable gap is reported as such, and the failing ledge is named.
    #[test]
    fn a_fourteen_hundred_pixel_gap_is_reported_unprovable_with_the_ledge_it_dies_on() {
        let problem = two_ledges(1400.0);
        match prove(&problem) {
            Verdict::Reachable(route) => {
                panic!("a 1400 px gap is beyond a 450 px dash, yet it claimed: {:?}", route.moves)
            }
            Verdict::Unreachable(frontier) => {
                assert!(
                    frontier.deepest.x > 400.0 && frontier.deepest.x < 700.0,
                    "the route dies at the end of the left ledge, not at ({:.0}, {:.0})",
                    frontier.deepest.x,
                    frontier.deepest.y,
                );
                assert!(frontier.distance_to_exit > 900.0);
                assert!(frontier.reachable_footholds > 0, "the left ledge itself is reachable");
                let text = explain(&frontier, problem.exit);
                assert!(text.contains("The route dies on the ledge at"), "got: {text}");
                assert!(text.contains(&format!("{:.0}", cfg::DASH_DISTANCE)), "got: {text}");

                // The far ledge is a foothold the character cannot get to, and saying how
                // far away it is, is the whole point: that is the number the model needs to
                // close the gap.
                let (at, distance) = frontier
                    .nearest_unreached
                    .expect("the far ledge is a foothold that was never reached");
                assert!(
                    at.x > 1900.0,
                    "the nearest unreached foothold is on the far ledge, not at ({:.0}, {:.0})",
                    at.x,
                    at.y
                );
                assert!(
                    (900.0..1600.0).contains(&distance),
                    "a 1400 px gap puts it roughly that far away, not {distance:.0} px"
                );
                assert!(
                    text.contains("The nearest foothold beyond it is"),
                    "explain must say how far the nearest foothold beyond the ledge lies: {text}"
                );
                assert!(text.contains(&format!("{distance:.0} px away")), "got: {text}");
            }
        }
    }

    // A9, first move — a gap only a dash clears.
    //
    // The width matters, and guessing it was wrong the first time: a *running* jump carries
    // far further than JUMP_HEIGHT suggests, because the character is moving at
    // PLAYER_RUN_SPEED 600 for the whole arc. Measured against this prover, a plain jump
    // clears up to about 800 px and a jump with an air dash up to about 1100 px, so 1000 px
    // is a gap that genuinely needs the dash. Rather than trust that arithmetic, the test
    // proves it both ways: the same gap is crossed with the full repertoire and uncrossable
    // with every dash move taken out of it.
    #[test]
    fn a_gap_that_only_a_dash_clears_is_proven_and_needs_the_dash_to_be() {
        let problem = two_ledges(1000.0);
        let full = primitives_for(&problem.world);
        let without_dash: Vec<Primitive> =
            full.iter().filter(|p| !p.name.contains("dash")).cloned().collect();
        assert!(
            without_dash.len() < full.len(),
            "the filter must actually remove the dash moves"
        );

        match prove_with(&problem, &full) {
            Verdict::Reachable(route) => {
                let landed = replay(&problem, &route);
                assert!(
                    (landed - problem.exit).length() <= ARRIVAL_RADIUS,
                    "the dash proof must replay to the far ledge"
                );
            }
            Verdict::Unreachable(f) => {
                panic!("a 1000 px gap is within reach with a dash: {}", explain(&f, problem.exit))
            }
        }

        assert!(
            !prove_with(&problem, &without_dash).is_reachable(),
            "with no dash in the repertoire the same 1000 px gap must be uncrossable — \
             otherwise this fixture is not testing the dash at all"
        );
    }

    // A9, second move — a shaft that only a wall jump climbs.
    #[test]
    fn a_shaft_taller_than_a_jump_is_climbed_by_wall_jumping_between_its_faces() {
        // Two facing walls a wall-jump apart, rising well past a single JUMP_HEIGHT, with
        // a floor at the top flush with them. Nothing but repeated wall jumps gets up it.
        let floor_y = 1600.0;
        let top_y = 700.0; // 900 px of rise: far beyond JUMP_HEIGHT 365.
        let problem = Problem {
            world: World {
                polys: vec![
                    rect(0.0, floor_y, 900.0, floor_y + 200.0, false),
                    // Left and right faces of the shaft, 150 px apart.
                    rect(300.0, top_y, 360.0, floor_y, false),
                    rect(510.0, top_y, 570.0, floor_y, false),
                    // The floor at the top, level with the top of the right-hand face so
                    // that arriving up the shaft and walking on is one continuous surface.
                    rect(570.0, top_y, 1000.0, top_y + 60.0, false),
                ],
                ropes: vec![],
            },
            spawn: Pos2::new(435.0, floor_y - 200.0),
            exit: Pos2::new(800.0, top_y - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
            level_size: Some(Vec2::new(1200.0, 2000.0)),
        };
        match prove(&problem) {
            Verdict::Reachable(route) => {
                let landed = replay(&problem, &route);
                assert!(
                    (landed - problem.exit).length() <= ARRIVAL_RADIUS,
                    "the wall-jump proof must replay to the top"
                );
            }
            Verdict::Unreachable(f) => panic!("the shaft must be climbable: {}", explain(&f, problem.exit)),
        }

        // The faces are what make it possible: take them away and the same 900 px rise,
        // far beyond JUMP_HEIGHT 365, becomes unreachable. That is what makes this a test
        // of wall jumping rather than of jumping.
        let mut without_faces = problem.clone();
        without_faces.world.polys.retain(|p| {
            let top = p.points.iter().fold(f32::INFINITY, |a, b| a.min(b.y));
            top > top_y + 1.0 || p.points.iter().any(|q| q.x > 900.0)
        });
        assert!(
            !prove(&without_faces).is_reachable(),
            "with nothing to wall jump off, a 900 px rise must be out of reach"
        );
    }

    // A9, third move — climbing a `wall_tool` face.
    //
    // Note on what this can and cannot assert. A "climb-only" geometry does not exist in
    // this character's kit: every wall can be wall-slid on, and a wall jump gains
    // WALL_JUMP_HEIGHT whether or not the face is `wall_tool`, so a lone plain wall can be
    // ratcheted up by jumping off it and returning to it higher. Comparing reachability
    // with and without `wall_tool` therefore proves nothing. What is worth proving, and is
    // proven here, is that the prover really does climb: that the climb move gains height
    // on a `wall_tool` face and gains none on the same face without it, and that a route
    // up such a face is found and replays.
    #[test]
    fn climbing_gains_height_on_a_wall_tool_face_and_none_on_a_plain_one() {
        let face = |climbable: bool| World {
            polys: vec![
                rect(0.0, 1400.0, 500.0, 1600.0, false),
                rect(500.0, 400.0, 560.0, 1600.0, climbable),
            ],
            ropes: vec![],
        };
        // Get the character clinging to the face: stand beside it and press into it.
        let cling_to = |world: &World| {
            let mut sim = Simulation::new(Pos2::new(440.0, 1300.0));
            let into = held(false, true, false, false, false, false);
            for _ in 0..frames(2.0) {
                sim.step(world, into, FIXED_DT);
            }
            // Jump up against the face so it is clinging rather than standing.
            for _ in 0..frames(0.3) {
                sim.step(world, held(false, true, false, false, true, false), FIXED_DT);
            }
            for _ in 0..frames(0.6) {
                sim.step(world, into, FIXED_DT);
            }
            sim
        };

        let mut heights = Vec::new();
        for climbable in [true, false] {
            let world = face(climbable);
            let mut sim = cling_to(&world);
            assert!(
                matches!(sim.state, State::WallSliding | State::Climbing),
                "the character should be on the face (climbable = {climbable}), not {:?}",
                sim.state
            );
            let before = sim.pos.y;
            let up = held(false, true, true, false, false, false);
            for _ in 0..frames(1.5) {
                sim.step(&world, up, FIXED_DT);
            }
            heights.push(before - sim.pos.y); // Y is down, so a rise is positive here.
        }

        let climbed = heights[0];
        let plain = heights[1];
        assert!(
            climbed > 100.0,
            "holding into a wall_tool face and pressing up must climb it, but it rose {climbed:.0} px"
        );
        assert!(
            plain <= 0.0,
            "the same face without wall_tool cannot be climbed, yet it rose {plain:.0} px"
        );
    }

    #[test]
    fn a_route_up_a_climbable_face_is_proven_and_replays() {
        let floor_y = 1400.0;
        let top_y = 800.0; // 600 px of rise: well beyond JUMP_HEIGHT 365.
        let problem = Problem {
            world: World {
                polys: vec![
                    rect(0.0, floor_y, 500.0, floor_y + 200.0, false),
                    rect(500.0, top_y, 560.0, floor_y + 200.0, true),
                    // The floor at the top, level with the top of the face being climbed.
                    rect(560.0, top_y, 1000.0, top_y + 60.0, false),
                ],
                ropes: vec![],
            },
            spawn: Pos2::new(250.0, floor_y - 200.0),
            exit: Pos2::new(800.0, top_y - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
            level_size: Some(Vec2::new(1200.0, 1800.0)),
        };
        match prove(&problem) {
            Verdict::Reachable(route) => {
                let landed = replay(&problem, &route);
                assert!(
                    (landed - problem.exit).length() <= ARRIVAL_RADIUS,
                    "the proof must replay to the top of the face"
                );
            }
            Verdict::Unreachable(f) => {
                panic!("a climbable face must give a way up: {}", explain(&f, problem.exit))
            }
        }
    }

    // The brief's verification expectations list a rope case among the prover's hand-built
    // levels, and it was missing: only that a rope move is *offered* when ropes exist was
    // checked, never that the prover can actually use one. Same shape as the dash case —
    // proven with the rope in the world, unprovable once it is taken away.
    #[test]
    fn a_gap_crossed_by_a_rope_is_proven_and_needs_the_rope_to_be() {
        // A 1400 px gap — past a jump, past a dash, and the same width A8 shows is
        // unprovable — with a rope hanging where the character can actually reach it.
        //
        // Where the rope hangs matters, and getting it wrong is how this test first failed:
        // a rope over the *middle* of a gap this wide is unreachable, because the character
        // has to grab it in mid-flight. Measured against this prover, the rope has to hang
        // within about 150 to 450 px of the ledge it is jumped from, with its lower end near
        // the floor's height, so there is rope to overlap on the way past.
        let left_end = 600.0;
        let gap = 1400.0;
        let right_start = left_end + gap;
        let floor_y = 1200.0;
        let anchor_y = 400.0;
        let rope_bottom = 1250.0;
        let world_with = World {
            polys: vec![
                rect(0.0, floor_y, left_end, floor_y + 200.0, false),
                rect(right_start, floor_y, right_start + 800.0, floor_y + 200.0, false),
            ],
            ropes: vec![crate::sim::Rope {
                anchor: Pos2::new(left_end + 250.0, anchor_y),
                length: rope_bottom - anchor_y,
            }],
        };
        let problem = Problem {
            world: world_with,
            spawn: Pos2::new(300.0, 1100.0),
            exit: Pos2::new(right_start + 300.0, floor_y - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
            level_size: Some(Vec2::new(right_start + 1000.0, 1800.0)),
        };

        let verdict = prove(&problem);
        let route = match verdict {
            Verdict::Reachable(route) => route,
            Verdict::Unreachable(f) => panic!(
                "a rope over the middle of the gap should carry the character across: {}",
                explain(&f, problem.exit)
            ),
        };
        let landed = replay(&problem, &route);
        assert!(
            (landed - problem.exit).length() <= ARRIVAL_RADIUS,
            "the rope proof must replay to the far ledge, but finished {:.0} px away",
            (landed - problem.exit).length()
        );

        // Take the rope away and the same geometry is impassable, so it really is the rope
        // doing the work and not some other move.
        let mut without = problem.clone();
        without.world.ropes.clear();
        assert!(
            !prove(&without).is_reachable(),
            "a {gap:.0} px gap with no rope is past a jump and a dash, so it must be unprovable"
        );
    }

    #[test]
    fn a_level_with_nowhere_else_to_stand_says_so_rather_than_inventing_a_distance() {
        // One ledge, an exit in mid-air above it: the route dies, and there is genuinely no
        // other foothold to name.
        let problem = Problem {
            world: World { polys: vec![rect(0.0, 800.0, 600.0, 1000.0, false)], ropes: vec![] },
            spawn: Pos2::new(300.0, 700.0),
            exit: Pos2::new(300.0, 100.0),
            level_size: Some(Vec2::new(1000.0, 1200.0)),
        };
        match prove(&problem) {
            Verdict::Unreachable(frontier) => {
                assert_eq!(
                    frontier.nearest_unreached, None,
                    "the only ledge is the one already being stood on"
                );
                let text = explain(&frontier, problem.exit);
                assert!(
                    text.contains("no other foothold anywhere in the level"),
                    "got: {text}"
                );
            }
            Verdict::Reachable(_) => panic!("600 px straight up is out of reach"),
        }
    }

    #[test]
    fn a_spawn_over_nothing_at_all_is_unreachable_not_a_crash() {
        let problem = Problem {
            world: World { polys: vec![], ropes: vec![] },
            spawn: Pos2::new(100.0, 100.0),
            exit: Pos2::new(900.0, 100.0),
            level_size: Some(Vec2::new(1000.0, 1000.0)),
        };
        match prove(&problem) {
            Verdict::Unreachable(f) => assert_eq!(f.reachable_footholds, 0),
            Verdict::Reachable(_) => panic!("there is no floor, so there is no route"),
        }
    }

    #[test]
    fn standing_on_the_exit_is_proven_without_any_moves() {
        let problem = Problem {
            world: World { polys: vec![rect(0.0, 800.0, 1000.0, 1000.0, false)], ropes: vec![] },
            spawn: Pos2::new(500.0, 700.0),
            exit: Pos2::new(500.0, 755.0),
            level_size: Some(Vec2::new(1000.0, 1200.0)),
        };
        match prove(&problem) {
            Verdict::Reachable(route) => {
                assert!(route.moves.is_empty(), "no move is needed, only the fall to the floor");
                // The route is not empty: falling to the floor is its first segment, and
                // replaying it must still land on the exit.
                assert!(route_arrives(&problem, &route), "and replaying it arrives");
            }
            Verdict::Unreachable(f) => panic!("the exit is underfoot: {}", explain(&f, problem.exit)),
        }
    }

    /// Every proof this prover produces must survive being replayed. This is the general
    /// form of a bug the rope fixture found: `apply` was settling the character with
    /// no-input frames that never made it into the recorded route, so replaying a route
    /// landed somewhere else. Cheap to assert across every fixture, and it would have caught
    /// that immediately.
    #[test]
    fn every_route_this_prover_finds_survives_being_replayed() {
        let mut problems = vec![two_ledges(200.0), two_ledges(1000.0)];
        // The rope fixture, which is the one that exposed the unrecorded settle frames: the
        // flat fixtures land before the settle matters, so without this the guard could pass
        // even with the fix reverted.
        {
            let left_end = 600.0;
            let right_start = left_end + 1400.0;
            problems.push(Problem {
                world: World {
                    polys: vec![
                        rect(0.0, 1200.0, left_end, 1400.0, false),
                        rect(right_start, 1200.0, right_start + 800.0, 1400.0, false),
                    ],
                    ropes: vec![crate::sim::Rope {
                        anchor: Pos2::new(left_end + 250.0, 400.0),
                        length: 850.0,
                    }],
                },
                spawn: Pos2::new(300.0, 1100.0),
                exit: Pos2::new(right_start + 300.0, 1200.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
                level_size: Some(Vec2::new(right_start + 1000.0, 1800.0)),
            });
        }
        // The climbable face and the wall-jump shaft, rebuilt here so this test covers the
        // awkward routes and not only the flat ones.
        problems.push(Problem {
            world: World {
                polys: vec![
                    rect(0.0, 1400.0, 500.0, 1600.0, false),
                    rect(500.0, 800.0, 560.0, 1600.0, true),
                    rect(560.0, 800.0, 1000.0, 860.0, false),
                ],
                ropes: vec![],
            },
            spawn: Pos2::new(250.0, 1200.0),
            exit: Pos2::new(800.0, 800.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
            level_size: Some(Vec2::new(1200.0, 1800.0)),
        });
        problems.push(Problem {
            world: World {
                polys: vec![
                    rect(0.0, 1600.0, 900.0, 1800.0, false),
                    rect(300.0, 700.0, 360.0, 1600.0, false),
                    rect(510.0, 700.0, 570.0, 1600.0, false),
                    rect(570.0, 700.0, 1000.0, 760.0, false),
                ],
                ropes: vec![],
            },
            spawn: Pos2::new(435.0, 1400.0),
            exit: Pos2::new(800.0, 700.0 - cfg::PLAYER_CAPSULE_HALF_EXTENT_Y),
            level_size: Some(Vec2::new(1200.0, 2000.0)),
        });

        for (i, problem) in problems.iter().enumerate() {
            match prove(problem) {
                Verdict::Reachable(route) => assert!(
                    route_arrives(problem, &route),
                    "fixture {i}: the route was proven but replaying it finished {:.0} px from \
                     the exit, so the recorded inputs are not what the search actually did",
                    (replay(problem, &route) - problem.exit).length()
                ),
                Verdict::Unreachable(_) => panic!("fixture {i} should be provable"),
            }
        }
    }

    #[test]
    fn the_prover_is_deterministic() {
        let problem = two_ledges(200.0);
        let a = prove(&problem);
        let b = prove(&problem);
        assert_eq!(a, b, "the same level must give the same verdict and the same route");
    }

    #[test]
    fn the_repertoire_covers_every_ability_the_character_has() {
        let with_rope = World {
            polys: vec![],
            ropes: vec![crate::sim::Rope { anchor: Pos2::new(0.0, 0.0), length: 200.0 }],
        };
        let names: Vec<&str> = primitives_for(&with_rope).iter().map(|p| p.name).collect();
        for expected in [
            "run left", "run right", "jump left", "jump right", "dash left", "dash right",
            "jump-dash left", "jump-dash right", "wall jump off left wall",
            "wall jump off right wall", "climb up", "climb down", "rope swing",
        ] {
            assert!(names.contains(&expected), "the repertoire is missing {expected:?}");
        }

        let without_rope = World { polys: vec![], ropes: vec![] };
        let plain: Vec<&str> = primitives_for(&without_rope).iter().map(|p| p.name).collect();
        assert!(
            !plain.contains(&"rope swing"),
            "a level with no ropes must not be told it can swing on one"
        );
    }
}
