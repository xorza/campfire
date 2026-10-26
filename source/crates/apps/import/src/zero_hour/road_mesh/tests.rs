use campfire_client_files::GroundQuad;

use super::*;

/// Heights of `width` samples a row, rows from the south, of no border.
fn heights(width: usize, rows: &[&[u8]]) -> MapHeights {
    MapHeights {
        width,
        depth: rows.len(),
        border: 0,
        boundary: None,
        samples: rows.concat(),
    }
}

/// The terrain's quads over the cells of `heights`, each split as `flip`, the file's, says.
fn quads(heights: &MapHeights, flip: bool) -> TerrainQuads {
    let cells = (heights.width - 1) * (heights.depth - 1);
    TerrainQuads {
        columns: u32::try_from(heights.width).unwrap(),
        ground: vec![
            GroundQuad {
                uv: [Vec2::ZERO; 4],
                flip,
            };
            cells
        ],
        overlays: Vec::new(),
    }
}

/// The piece of a road from `start` to `end` along `x`, of a type `scale` units wide, all of it
/// drawn across, its texture from `(0, 85 / 512)` at its start.
fn piece(start: Vec2, end: Vec2, scale: f32) -> RoadPiece {
    let normal = Vec2::new(0.0, scale / 2.0);
    RoadPiece {
        corners: PieceCorners {
            start_right: start - normal,
            start_left: start + normal,
            end_right: end - normal,
            end_left: end + normal,
        },
        origin: start,
        uv: Vec2::new(0.0, 85.0 / 512.0),
        along: end - start,
        normal,
        scale: Vec2::splat(scale),
    }
}

#[test]
fn a_piece_lies_on_the_triangles_its_cells_ground_quads_split_them_in() {
    // One cell, its north-east corner 16 steps high, 10 units, the rest 0. A road 10 wide from
    // (0, 5) to (10, 5) covers it whole, as the triangles north-west, south-west, south-east,
    // and north-west, south-east, north-east.
    let heights = heights(2, &[&[0, 0], &[0, 16]]);
    let road = piece(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0), 10.0);
    let mut mesh = RoadMesh::default();

    // A quad the file flips not meets on the game's diagonal from south-east to north-west, as
    // the road's own: the road's first triangle lies on the cell's south-west one, of height 0
    // at each corner, and its second on the north-east one, 10 at its north-east corner. The
    // other triangle of each has no area, and drops.
    let unflipped = quads(&heights, false);
    mesh.lay(
        &road,
        RoadGround {
            heights: &heights,
            quads: &unflipped,
        },
    );
    let float = 0.078125;
    assert_eq!(
        mesh.points,
        [
            [0.0, 10.0, float],
            [0.0, 0.0, float],
            [10.0, 0.0, float],
            [0.0, 10.0, float],
            [10.0, 0.0, float],
            [10.0, 10.0, 10.0 + float],
        ]
        .map(Vec3::from_array)
    );
    assert_eq!(mesh.triangles, [[0, 1, 2], [3, 4, 5]]);
    // A repeat is 4 · 10 = 40 long: `u` is x / 40, and `v` 85 / 512 less (y − 5) / 40, so
    // 85 / 512 ± 64 / 512 at the road's sides.
    let [south, north] = [149.0 / 512.0, 21.0 / 512.0];
    assert_eq!(
        mesh.uvs,
        [
            [0.0, north],
            [0.0, south],
            [0.25, south],
            [0.0, north],
            [0.25, south],
            [0.25, north],
        ]
        .map(Vec2::from_array)
    );

    // A quad the file flips meets on the diagonal from south-west to north-east, which cuts
    // each of the road's triangles at the cell's middle, (5, 5): each part lies on its cell
    // triangle, the south-east one 16 · y / 10 steps high, the north-west one 16 · x / 10, so
    // the middle stands 8 steps, 5 units, high, where the first split laid it at 0.
    let flipped = quads(&heights, true);
    mesh.lay(
        &road,
        RoadGround {
            heights: &heights,
            quads: &flipped,
        },
    );
    assert_eq!(
        mesh.points,
        [
            [0.0, 0.0, float],
            [10.0, 0.0, float],
            [5.0, 5.0, 5.0 + float],
            [0.0, 10.0, float],
            [0.0, 0.0, float],
            [5.0, 5.0, 5.0 + float],
            [5.0, 5.0, 5.0 + float],
            [10.0, 0.0, float],
            [10.0, 10.0, 10.0 + float],
            [0.0, 10.0, float],
            [5.0, 5.0, 5.0 + float],
            [10.0, 10.0, 10.0 + float],
        ]
        .map(Vec3::from_array)
    );
    assert_eq!(
        mesh.triangles,
        [[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]]
    );
    assert_eq!(mesh.uvs[2], Vec2::new(0.125, 85.0 / 512.0));
    for triangle in &mesh.triangles {
        let [a, b, c] = triangle.map(|at| mesh.points[at as usize]);
        assert!((b - a).cross(c - a).z > 0.0, "{triangle:?} faces up");
    }
}

#[test]
fn a_piece_over_a_slope_lies_on_it_and_past_the_grid_on_its_edge() {
    // Two cells east, the ground climbing 8 steps, 5 units, a cell: 0.5 · x high. A road 4
    // wide from (5, 5) to (25, 5) runs over both cells' triangles, then 5 units past the
    // grid's east edge, where the ground keeps the edge's 10.
    let heights = heights(3, &[&[0, 8, 16], &[0, 8, 16]]);
    let road = piece(Vec2::new(5.0, 5.0), Vec2::new(25.0, 5.0), 4.0);
    let mut mesh = RoadMesh::default();
    for flip in [false, true] {
        let quads = quads(&heights, flip);
        mesh.lay(
            &road,
            RoadGround {
                heights: &heights,
                quads: &quads,
            },
        );
        // A crossing of the road's diagonal falls between `f32`'s points, so its height may
        // round an ulp or two off the slope's, at most 2⁻²⁰ at heights from 8 to 16.
        for point in &mesh.points {
            let ground = 0.5 * point.x.min(20.0) + 0.078125;
            assert!((point.z - ground).abs() <= 2.0_f32.powi(-20), "{point}");
        }
        // The parts cover the road's 20 · 4 = 80 once.
        let area: f32 = mesh
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.map(|at| mesh.points[at as usize].truncate());
                (b - a).perp_dot(c - a) / 2.0
            })
            .sum();
        assert_eq!(area, 80.0);
        // Past the edge, the first triangle's part keeps its corner (25, 3), and the second's
        // its corners (25, 3) and (25, 7).
        let past = mesh.points.iter().filter(|point| point.x > 20.0).count();
        assert_eq!(past, 3);
    }
}
