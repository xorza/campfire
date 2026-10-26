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
fn a_column_stands_at_the_highest_corner_under_its_samples_plus_an_eighth_of_a_step() {
    // A road 20 wide from (0, 10) to (20, 10): 20 / 10 + 1 = 3 columns, at x 0, 10 and 20, each
    // of 20 / 10 + 1 = 3 samples across, at y 0, 10 and 20, in the cells of rows 0, 1 and 2.
    // Column 0's cells hold no height; column 1's right sample, in the cell of columns 1 and 2
    // and rows 0 and 1, finds 8 steps at (2, 1), 5 units; column 2's left sample, in the cell of
    // columns 2 and 3 and rows 2 and 3, finds 12 at (3, 3), 7.5. The line from column 0 to 2
    // passes 3.75 at column 1, below its 5, so it stays.
    let rows: [&[u8]; 4] = [&[0, 0, 0, 0], &[0, 0, 8, 0], &[0, 0, 0, 0], &[0, 0, 0, 12]];
    let heights = heights(4, &rows);
    let mut mesh = RoadMesh::default();
    let road = piece(Vec2::new(0.0, 10.0), Vec2::new(20.0, 10.0), 20.0);
    mesh.float(&road, RoadGround { heights: &heights });
    // Each column's right then left vertex, 1/8 of 0.625 = 0.078125 over its height.
    assert_eq!(
        mesh.points,
        [
            [0.0, 0.0, 0.078125],
            [0.0, 20.0, 0.078125],
            [10.0, 0.0, 5.078125],
            [10.0, 20.0, 5.078125],
            [20.0, 0.0, 7.578125],
            [20.0, 20.0, 7.578125],
        ]
        .map(Vec3::from_array)
    );
    // A repeat is 4 · 20 = 80 long: `u` is x / 80, and `v` 85 / 512 less the left offset over
    // 80, 85 / 512 ± 64 / 512.
    let [right, left] = [149.0 / 512.0, 21.0 / 512.0];
    assert_eq!(
        mesh.uvs,
        [
            [0.0, right],
            [0.0, left],
            [0.125, right],
            [0.125, left],
            [0.25, right],
            [0.25, left],
        ]
        .map(Vec2::from_array)
    );
    assert_eq!(mesh.triangles, [[1, 0, 2], [1, 2, 3], [3, 2, 4], [3, 4, 5]]);
    for triangle in &mesh.triangles {
        let [a, b, c] = triangle.map(|at| mesh.points[at as usize]);
        assert!((b - a).cross(c - a).z > 0.0, "{triangle:?} faces up");
    }

    // Past the map, a point's cell is clamped to the last: a road 10 long beyond the east edge,
    // of the least two columns, stands on the last cells' highest corner, 7.5.
    let far = piece(Vec2::new(100.0, 10.0), Vec2::new(110.0, 10.0), 20.0);
    mesh.float(&far, RoadGround { heights: &heights });
    assert_eq!(mesh.points.len(), 4);
    assert!(mesh.points.iter().all(|point| point.z == 7.578125));
}

#[test]
fn a_column_the_line_past_it_passes_close_above_is_dropped_its_ends_weighed_as_the_game_does() {
    // A road 8 wide from (0, 5) to (40, 5): 5 columns, at x 0 to 40, each of 2 samples, at y 1
    // and 9, in row 0's cells. Column k's height is the higher of samples k and k + 1: 0, 0,
    // 1.25, 2.5 and 2.5.
    let row: &[u8] = &[0, 0, 0, 2, 4, 0];
    let heights = heights(6, &[row, row]);
    let mut mesh = RoadMesh::default();
    let road = piece(Vec2::new(0.0, 5.0), Vec2::new(40.0, 5.0), 8.0);
    mesh.float(&road, RoadGround { heights: &heights });
    // Column 1: the line from column 0 to 2 passes 0.625 there, at or above its 0 and within
    // 0.6875, so it is dropped. Column 2: the game weighs column 0 by its distance 2 and column
    // 3 by 1, (0 · 2 + 2.5 · 1) / 3 = 0.83, below its 1.25, so it stays, where a line would pass
    // (0 · 1 + 2.5 · 2) / 3 = 1.67, within 0.6875 above it. Column 3: (1.25 + 2.5) / 2 = 1.875,
    // below its 2.5.
    let kept: Vec<[f32; 2]> = mesh
        .points
        .iter()
        .step_by(2)
        .map(|point| [point.x, point.z])
        .collect();
    assert_eq!(
        kept,
        [
            [0.0, 0.078125],
            [20.0, 1.328125],
            [30.0, 2.578125],
            [40.0, 2.578125]
        ]
    );
    // A repeat of 32: `u` 20 / 32 = 0.625, 30 / 32 and 40 / 32, past one repeat.
    let u: Vec<f32> = mesh.uvs.iter().step_by(2).map(|uv| uv.x).collect();
    assert_eq!(u, [0.0, 0.625, 0.9375, 1.25]);
    assert_eq!(mesh.triangles.len(), 6);
    assert_eq!(mesh.triangles[4], [5, 4, 6]);
}
