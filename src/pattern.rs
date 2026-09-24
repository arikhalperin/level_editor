//! Filling a polygon with a tiled bitmap, and the gallery of patterns recently used.
//!
//! Everything here is pure — no egui context, no textures, no file system, no editor
//! state — so the triangulation, the tiling arithmetic and the gallery's recency rules
//! are all testable headlessly.

use egui::{Pos2, Vec2};

/// How many patterns the gallery remembers before the oldest falls off the end.
pub const GALLERY_CAPACITY: usize = 12;

/// Twice the signed area of the polygon, by the shoelace formula. The sign gives the
/// winding, which ear clipping needs; the magnitude is twice the area.
fn signed_area_2(points: &[Pos2]) -> f32 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    sum
}

/// Where a world point lands on the texture when one texture pixel covers one world
/// pixel. Values beyond 1 repeat, and negative ones repeat backwards, which is what
/// `TextureWrapMode::Repeat` turns into a tiled fill.
///
/// The tiling is measured from the world origin, so this depends on nothing but the
/// point and the texture: two polygons meeting at a vertex get the same answer, which is
/// what makes the pattern continue across their shared edge. A texture with no size has
/// no meaningful coordinate, so it yields the origin rather than dividing by zero.
pub fn uv_for(point: Pos2, texture_size: Vec2) -> Pos2 {
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return Pos2::ZERO;
    }
    Pos2::new(point.x / texture_size.x, point.y / texture_size.y)
}

/// Which side of the line `o -> a` the point `b` falls on.
fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// True when `p` is inside the triangle `a b c`, edges included.
fn inside_triangle(a: Pos2, b: Pos2, c: Pos2, p: Pos2) -> bool {
    let d1 = cross(a, b, p);
    let d2 = cross(b, c, p);
    let d3 = cross(c, a, p);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

/// Split a simple polygon into triangles by ear clipping, returning indices into
/// `points`. A polygon of `n` points yields `n - 2` triangles; fewer than three points,
/// or a degenerate outline with no area, yields none.
///
/// Ear clipping rather than a convex hull, so a concave blocker is filled inside the
/// outline that is actually drawn rather than across the notch.
pub fn triangulate(points: &[Pos2]) -> Vec<[usize; 3]> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let area_2 = signed_area_2(points);
    if area_2 == 0.0 {
        return Vec::new();
    }

    // Work anticlockwise by the shoelace sign, so a convex corner is always a positive
    // cross product and the ear test below needs only one orientation.
    let mut remaining: Vec<usize> = (0..n).collect();
    if area_2 < 0.0 {
        remaining.reverse();
    }

    let mut triangles = Vec::with_capacity(n - 2);
    // Each successful clip removes one vertex, so this many attempts is generous; the
    // counter exists only so a self-intersecting outline cannot spin here forever.
    let mut attempts = 0;
    let max_attempts = n * n;

    while remaining.len() > 3 && attempts < max_attempts {
        attempts += 1;
        let count = remaining.len();
        let mut clipped = false;

        for i in 0..count {
            let prev = remaining[(i + count - 1) % count];
            let cur = remaining[i];
            let next = remaining[(i + 1) % count];
            let (a, b, c) = (points[prev], points[cur], points[next]);

            // A reflex corner cannot be an ear.
            if cross(a, b, c) <= 0.0 {
                continue;
            }
            // Nor can one whose triangle swallows another vertex.
            let swallows = remaining
                .iter()
                .filter(|&&idx| idx != prev && idx != cur && idx != next)
                .any(|&idx| inside_triangle(a, b, c, points[idx]));
            if swallows {
                continue;
            }

            triangles.push([prev, cur, next]);
            remaining.remove(i);
            clipped = true;
            break;
        }

        if !clipped {
            // No ear found: the outline is self-intersecting or numerically degenerate.
            // Fall back to a fan so something sensible is still drawn.
            break;
        }
    }

    if remaining.len() == 3 {
        triangles.push([remaining[0], remaining[1], remaining[2]]);
    } else if remaining.len() > 3 {
        for i in 1..remaining.len() - 1 {
            triangles.push([remaining[0], remaining[i], remaining[i + 1]]);
        }
    }

    triangles
}

/// A polygon's fill, ready to become an `egui::Mesh`: screen-space positions paired
/// with texture coordinates, and the triangles joining them.
#[derive(Debug, Clone, PartialEq)]
pub struct FillMesh {
    /// One entry per polygon vertex, in the polygon's own order: where it sits on
    /// screen, and where that lands on the texture.
    pub vertices: Vec<(Pos2, Pos2)>,
    /// Indices into `vertices`.
    pub triangles: Vec<[usize; 3]>,
}

/// Build the fill for `points` using a texture of `texture_size`, with the view scrolled
/// by `scroll_offset`. `None` when there is nothing to fill: fewer than three points, an
/// outline enclosing no area, or a texture with no size.
pub fn fill_mesh(points: &[Pos2], texture_size: Vec2, scroll_offset: Vec2) -> Option<FillMesh> {
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
        return None;
    }
    let triangles = triangulate(points);
    if triangles.is_empty() {
        return None;
    }
    // Measured from the world origin rather than the shape's own corner, so every
    // polygon in the level sits on one tiling grid and two blockers that touch continue
    // the pattern across their shared edge instead of each restarting it. The cost is
    // that a pattern is fixed to the world: drag a blocker and the shape slides over the
    // pattern rather than carrying it along.
    let vertices = points
        .iter()
        .map(|p| (*p - scroll_offset, uv_for(*p, texture_size)))
        .collect();
    Some(FillMesh { vertices, triangles })
}

/// The patterns used most recently, newest first.
///
/// This is editor preference state, not level data: it never reaches a level file and
/// must never influence whether a level counts as having unsaved changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gallery {
    entries: Vec<String>,
}

impl Gallery {
    /// Build from stored paths, newest first, applying the same capacity and
    /// deduplication rules as `remember` so a hand-edited config cannot exceed them.
    pub fn from_paths<I, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut gallery = Self::default();
        // Oldest first into `remember`, so the stored order survives.
        let owned: Vec<String> = paths.into_iter().map(Into::into).collect();
        for path in owned.into_iter().rev() {
            gallery.remember(path);
        }
        gallery
    }

    /// The remembered paths, most recently used first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record a pattern as just used. An entry already present moves to the front rather
    /// than appearing twice, and the oldest falls off once the list is full.
    pub fn remember(&mut self, path: impl Into<String>) {
        let path = path.into();
        if path.is_empty() {
            return;
        }
        self.entries.retain(|e| e != &path);
        self.entries.insert(0, path);
        self.entries.truncate(GALLERY_CAPACITY);
    }

    /// Drop remembered patterns whose file `exists` rejects, so the gallery cannot
    /// accumulate entries that can no longer be drawn.
    pub fn retain_existing(&mut self, exists: impl Fn(&str) -> bool) {
        self.entries.retain(|e| exists(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The area the polygon encloses, whichever way its points wind. Only the tests
    /// need this: the fill itself works from the triangles, not from an area.
    fn area(points: &[Pos2]) -> f32 {
        signed_area_2(points).abs() / 2.0
    }

    fn square() -> Vec<Pos2> {
        vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(256.0, 0.0),
            Pos2::new(256.0, 128.0),
            Pos2::new(0.0, 128.0),
        ]
    }

    /// A square with a deep triangular notch cut into its top edge, down to (50, 20).
    /// The reflex corner is the notch apex.
    ///
    /// Chosen deliberately over an L: for an L, a naive fan from vertex 0 happens to
    /// total the same area as the true shape, so an area check alone cannot tell a
    /// correct triangulation from a wrong one. Here a fan spans the notch, so both the
    /// area and the centroid checks below actually discriminate.
    fn a_notched_shape() -> Vec<Pos2> {
        vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(50.0, 20.0),
            Pos2::new(0.0, 100.0),
        ]
    }

    /// Ray casting, so the tests can assert a triangle really sits inside the outline.
    fn contains(points: &[Pos2], p: Pos2) -> bool {
        let mut inside = false;
        let n = points.len();
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > p.x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    fn centroid(a: Pos2, b: Pos2, c: Pos2) -> Pos2 {
        Pos2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
    }

    fn triangle_area(a: Pos2, b: Pos2, c: Pos2) -> f32 {
        ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.0
    }

    fn triangulated_area(points: &[Pos2]) -> f32 {
        triangulate(points)
            .into_iter()
            .map(|[a, b, c]| triangle_area(points[a], points[b], points[c]))
            .sum()
    }

    /// The texture coordinates `fill_mesh` produces for `points`, in the polygon's own
    /// vertex order. The scroll offset is irrelevant to them, so it is zero here.
    fn uvs(points: &[Pos2], texture: Vec2) -> Vec<Pos2> {
        fill_mesh(points, texture, Vec2::ZERO)
            .expect("a real shape with a real texture fills")
            .vertices
            .into_iter()
            .map(|(_, uv)| uv)
            .collect()
    }

    #[test]
    fn a_texture_with_no_size_has_no_meaningful_coordinate() {
        // fill_mesh refuses such a texture before reaching here, but uv_for is public
        // and must not divide by zero for anyone who calls it directly.
        assert_eq!(uv_for(Pos2::new(100.0, 100.0), Vec2::ZERO), Pos2::ZERO);
        assert_eq!(uv_for(Pos2::new(100.0, 100.0), Vec2::new(64.0, 0.0)), Pos2::ZERO);
        assert_eq!(uv_for(Pos2::new(100.0, 100.0), Vec2::new(-64.0, 64.0)), Pos2::ZERO);
    }

    #[test]
    fn a_world_point_maps_to_the_same_coordinate_whichever_polygon_asks() {
        // The property the whole change rests on, stated directly rather than only
        // through two shapes that happen to meet.
        let texture = Vec2::new(64.0, 64.0);
        let shared_corner = Pos2::new(256.0, 128.0);
        assert_eq!(uv_for(shared_corner, texture), Pos2::new(4.0, 2.0));
        assert_eq!(
            uv_for(shared_corner, texture),
            uv_for(shared_corner, texture),
            "a world point's texture coordinate depends on nothing else"
        );
    }

    #[test]
    fn a_sixty_four_pixel_pattern_repeats_four_by_two_across_a_256_by_128_shape() {
        // A shape whose corner sits on the world origin is the one case where world
        // anchoring and shape anchoring agree, so this is also a regression guard.
        let uv = uvs(&square(), Vec2::new(64.0, 64.0));

        assert_eq!(uv[0], Pos2::new(0.0, 0.0), "the corner at the world origin starts the tiling");
        assert_eq!(uv[2].x, 4.0, "256 world pixels over a 64px texture is four tiles across");
        assert_eq!(uv[2].y, 2.0, "and 128 over 64 is two tiles down");
    }

    #[test]
    fn the_tiling_is_measured_from_the_world_not_the_shapes_own_corner() {
        let moved: Vec<Pos2> = square().iter().map(|p| *p + Vec2::new(1000.0, 500.0)).collect();
        let uv = uvs(&moved, Vec2::new(64.0, 64.0));

        assert_eq!(
            uv[0],
            Pos2::new(1000.0 / 64.0, 500.0 / 64.0),
            "a shape away from the origin starts partway through a tile, not at (0, 0)"
        );
        assert_ne!(uv[0], Pos2::new(0.0, 0.0), "which is exactly what shape anchoring would give");
    }

    #[test]
    fn two_blockers_sharing_an_edge_continue_the_pattern_across_it() {
        // The whole point: side by side, they must read as one surface.
        let texture = Vec2::new(64.0, 64.0);
        let left = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(256.0, 0.0),
            Pos2::new(256.0, 128.0),
            Pos2::new(0.0, 128.0),
        ];
        let right: Vec<Pos2> = left.iter().map(|p| *p + Vec2::new(256.0, 0.0)).collect();

        let left_uv = uvs(&left, texture);
        let right_uv = uvs(&right, texture);

        // The shared edge is x = 256: the left shape's second corner and the right
        // shape's first. Both must land on the same place in the texture.
        assert_eq!(left_uv[1].x, 4.0, "the left blocker reaches the seam four tiles along");
        assert_eq!(
            right_uv[0], left_uv[1],
            "and the right blocker starts there, so the pattern runs straight through"
        );
        assert_eq!(right_uv[1].x, 8.0, "continuing to eight tiles at its far edge");
    }

    #[test]
    fn moving_a_blocker_slides_it_over_a_pattern_fixed_to_the_world() {
        // The accepted cost of neighbours lining up, asserted so it cannot change
        // unnoticed: the pattern belongs to the level, not to the shape.
        let texture = Vec2::new(64.0, 64.0);
        let before = uvs(&square(), texture);
        let after = uvs(
            &square().iter().map(|p| *p + Vec2::new(32.0, 0.0)).collect::<Vec<_>>(),
            texture,
        );

        assert_ne!(before[0], after[0], "the shape moved, so it shows a different part of the pattern");
        assert_eq!(after[0], Pos2::new(0.5, 0.0), "half a tile along, because it moved half a tile");
    }

    #[test]
    fn a_concave_shape_is_filled_only_inside_its_outline() {
        let points = a_notched_shape();
        let triangles = triangulate(&points);

        assert_eq!(triangles.len(), points.len() - 2, "an n-gon clips to n - 2 triangles");
        assert_eq!(area(&points), 6000.0, "the notched shape's own area");

        // A fan across the notch would total 9000 here: half again as much as the shape.
        let filled = triangulated_area(&points);
        assert!(
            (filled - 6000.0).abs() < 0.01,
            "the triangles cover the shape exactly, not across the notch: got {filled}"
        );

        // And none of them strays outside, which is what "clipped to the outline" means.
        for [a, b, c] in triangulate(&points) {
            let middle = centroid(points[a], points[b], points[c]);
            assert!(
                contains(&points, middle),
                "triangle {:?} sits outside the outline at {middle:?}",
                [a, b, c]
            );
        }
    }

    #[test]
    fn triangulation_is_independent_of_winding() {
        let mut reversed = a_notched_shape();
        reversed.reverse();
        let filled = triangulated_area(&reversed);
        assert_eq!(triangulate(&reversed).len(), reversed.len() - 2);
        assert!((filled - 6000.0).abs() < 0.01, "clockwise points fill the same area");
        for [a, b, c] in triangulate(&reversed) {
            let middle = centroid(reversed[a], reversed[b], reversed[c]);
            assert!(contains(&reversed, middle), "and still inside the outline");
        }
    }

    #[test]
    fn a_degenerate_outline_fills_nothing() {
        assert!(triangulate(&[]).is_empty(), "no points");
        assert!(triangulate(&[Pos2::ZERO, Pos2::new(1.0, 1.0)]).is_empty(), "a line is not a shape");
        let collinear = vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(20.0, 0.0)];
        assert!(triangulate(&collinear).is_empty(), "three points on a line enclose nothing");
    }

    #[test]
    fn fill_mesh_pairs_every_vertex_with_its_texture_coordinate() {
        let points = square();
        let fill = fill_mesh(&points, Vec2::new(64.0, 64.0), Vec2::new(100.0, 50.0))
            .expect("a square with a real texture fills");

        assert_eq!(fill.vertices.len(), 4, "one entry per polygon vertex");
        assert_eq!(fill.triangles.len(), 2, "a quad is two triangles");
        // Screen position follows the editor's world = screen + scroll_offset rule.
        assert_eq!(fill.vertices[0].0, Pos2::new(-100.0, -50.0), "scrolled into screen space");
        assert_eq!(
            fill.vertices[0].1,
            Pos2::new(0.0, 0.0),
            "while the texture coordinate ignores the scroll entirely, so the pattern \
             stays put in the level instead of swimming as the view moves"
        );
        assert_eq!(fill.vertices[2].1, Pos2::new(4.0, 2.0));
    }

    #[test]
    fn nothing_fills_without_a_usable_texture_or_shape() {
        assert!(fill_mesh(&square(), Vec2::ZERO, Vec2::ZERO).is_none(), "a texture with no size");
        assert!(fill_mesh(&[], Vec2::new(64.0, 64.0), Vec2::ZERO).is_none(), "no points");
        let line = vec![Pos2::ZERO, Pos2::new(10.0, 0.0)];
        assert!(fill_mesh(&line, Vec2::new(64.0, 64.0), Vec2::ZERO).is_none(), "no area");
    }

    // ── Gallery ──────────────────────────────────────────────────────────────

    #[test]
    fn three_patterns_are_remembered_most_recent_first() {
        let mut g = Gallery::default();
        g.remember("/a.png");
        g.remember("/b.png");
        g.remember("/c.png");
        assert_eq!(g.entries(), ["/c.png", "/b.png", "/a.png"], "newest first");
    }

    #[test]
    fn reusing_a_pattern_moves_it_to_the_front_without_duplicating() {
        let mut g = Gallery::default();
        for p in ["/a.png", "/b.png", "/c.png"] {
            g.remember(p);
        }
        // "/a.png" is now the third entry.
        assert_eq!(g.entries()[2], "/a.png");
        g.remember("/a.png");

        assert_eq!(g.entries(), ["/a.png", "/c.png", "/b.png"], "moved to the front");
        assert_eq!(g.entries().len(), 3, "and not added twice");
        assert!(g.entries().len() <= GALLERY_CAPACITY);
    }

    #[test]
    fn a_thirteenth_pattern_evicts_the_oldest() {
        let mut g = Gallery::default();
        for i in 0..GALLERY_CAPACITY {
            g.remember(format!("/p{i}.png"));
        }
        assert_eq!(g.entries().len(), GALLERY_CAPACITY);
        assert_eq!(g.entries().last().unwrap(), "/p0.png", "the first one used is oldest");

        g.remember("/new.png");
        assert_eq!(g.entries().len(), GALLERY_CAPACITY, "still exactly twelve");
        assert_eq!(g.entries()[0], "/new.png");
        assert!(!g.entries().contains(&"/p0.png".to_string()), "the oldest fell off");
        assert_eq!(g.entries().last().unwrap(), "/p1.png", "and the next oldest is now last");
    }

    #[test]
    fn stored_order_survives_a_round_trip_through_from_paths() {
        let mut g = Gallery::default();
        for p in ["/a.png", "/b.png", "/c.png"] {
            g.remember(p);
        }
        let stored: Vec<String> = g.entries().to_vec();
        assert_eq!(Gallery::from_paths(stored), g, "reading back gives the same gallery");
    }

    #[test]
    fn from_paths_applies_the_same_capacity_and_deduplication_rules() {
        let too_many: Vec<String> = (0..20).map(|i| format!("/p{i}.png")).collect();
        let g = Gallery::from_paths(too_many);
        assert_eq!(g.entries().len(), GALLERY_CAPACITY, "a hand-edited config cannot exceed the cap");
        assert_eq!(g.entries()[0], "/p0.png", "and the stored order is kept");

        let duplicated = vec!["/a.png".to_string(), "/b.png".to_string(), "/a.png".to_string()];
        let g = Gallery::from_paths(duplicated);
        assert_eq!(g.entries(), ["/a.png", "/b.png"], "duplicates collapse");
    }

    #[test]
    fn patterns_whose_file_has_gone_are_dropped() {
        let mut g = Gallery::default();
        for p in ["/gone.png", "/here.png", "/also_gone.png"] {
            g.remember(p);
        }
        g.retain_existing(|p| p == "/here.png");
        assert_eq!(g.entries(), ["/here.png"], "only what still exists is offered");
    }

    #[test]
    fn an_empty_path_is_never_remembered() {
        let mut g = Gallery::default();
        g.remember("");
        assert!(g.is_empty(), "nothing to show for an empty path");
    }
}
