use campfire_client_files::{GroundQuad, Overlay};
use campfire_math::Num;

use super::*;

/// 3 × 3 samples 10 m apart, a step of 0.625 m, the first at `x = −10`, `z = −20`: so the
/// sample `[column, row]` lies at `x = −10 + 10 · column`, `z = −20 + 10 · row`. Its heights,
/// in steps and in meters:
///
/// ```text
/// row 0:  0 16 32    0 10 20
/// row 1:  0 16 32    0 10 20
/// row 2:  0  0  0    0  0  0
/// ```
fn grid() -> HeightGrid {
    HeightGrid::new(
        [Num::from_int(-10).unwrap(), Num::from_int(-20).unwrap()],
        Num::from_int(10).unwrap(),
        Num::from_bits(10 << (Num::FRAC_BITS - 4)),
        3,
        vec![0, 16, 32, 0, 16, 32, 0, 0, 0],
    )
    .unwrap()
}

/// The ground of the grid's four cells, cell 3 flipped, and an overlay over cell 3; a texture
/// of 64 × 32 texels.
fn quads() -> TerrainQuads {
    let square = |corner: Vec2| {
        [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y].map(|offset| corner + 16.0 * offset)
    };
    let ground = [
        Vec2::ZERO,
        Vec2::new(16.0, 0.0),
        Vec2::new(0.0, 16.0),
        Vec2::splat(16.0),
    ]
    .into_iter()
    .enumerate()
    .map(|(cell, corner)| GroundQuad {
        uv: square(corner),
        flip: cell == 3,
    })
    .collect();
    TerrainQuads {
        columns: 3,
        ground,
        overlays: vec![Overlay {
            cell: 3,
            uv: square(Vec2::new(32.0, 16.0)),
            alpha: [0, 255, 51, 0],
            flip: false,
        }],
    }
}

fn close(found: Vec3, expected: Vec3) -> bool {
    // A normal is a square root's quotient, which an f32 rounds.
    found.abs_diff_eq(expected, 1e-6)
}

#[test]
fn a_quad_lies_on_its_cell_s_samples_with_their_normals_its_coordinates_and_its_diagonal() {
    let meshes = TerrainMeshes::of(&quads(), UVec2::new(64, 32), &grid());
    let ground = &meshes.ground;
    assert_eq!(ground.positions.len(), 16);
    // Cell 0's corners: the samples [0, 0], [1, 0], [1, 1] and [0, 1].
    assert_eq!(
        ground.positions[..4],
        [
            Vec3::new(-10.0, 0.0, -20.0),
            Vec3::new(0.0, 10.0, -20.0),
            Vec3::new(0.0, 10.0, -10.0),
            Vec3::new(-10.0, 0.0, -10.0)
        ]
    );
    // Its coordinates, texels over 64 × 32: 16 is 0.25 across and 0.5 down.
    assert_eq!(
        ground.uvs[..4],
        [
            Vec2::ZERO,
            Vec2::new(0.25, 0.0),
            Vec2::new(0.25, 0.5),
            Vec2::new(0.0, 0.5)
        ]
    );
    // [0, 0]: east 10 m over the one cell to [1, 0], the edge clamping the west; south level.
    // So the slopes are 1 and 0, and the normal (−1, 1, 0) / √2.
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert!(close(ground.normals[0], Vec3::new(-half, half, 0.0)));
    // [1, 0]: 20 m over the two cells; [1, 1]: 20 m up east over two, and 10 m down south
    // over two: (−1, 1, 0.5) / 1.5.
    assert!(close(ground.normals[1], Vec3::new(-half, half, 0.0)));
    assert!(close(
        ground.normals[2],
        Vec3::new(-2.0 / 3.0, 2.0 / 3.0, 1.0 / 3.0)
    ));
    // Cell 3's corners: [1, 1], [2, 1], [2, 2] and [1, 2]. [2, 1]: 10 m up east over one
    // cell, 20 m down south over two: (−1, 1, 1) / √3; [2, 2]: level east, 20 m down over
    // one: (0, 1, 2) / √5; [1, 2]: level east, 10 m down over one: (0, 1, 1) / √2.
    let third = 1.0 / 3.0_f32.sqrt();
    let fifth = 1.0 / 5.0_f32.sqrt();
    assert!(close(ground.normals[13], Vec3::new(-third, third, third)));
    assert!(close(
        ground.normals[14],
        Vec3::new(0.0, fifth, 2.0 * fifth)
    ));
    assert!(close(ground.normals[15], Vec3::new(0.0, half, half)));
    assert_eq!(ground.positions[14], Vec3::new(10.0, 0.0, 0.0));
    // Cell 0 meets on its corners 0 and 2, cell 3, flipped, on 1 and 3.
    assert_eq!(ground.indices[..6], [0, 2, 1, 0, 3, 2]);
    assert_eq!(ground.indices[18..], [12, 15, 13, 13, 15, 14]);
    // Every triangle faces up.
    for triangle in ground.indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|at| ground.positions[at as usize]);
        assert!((b - a).cross(c - a).y > 0.0, "{triangle:?}");
    }
    assert!(ground.colors.is_empty());
    // The overlay lies on cell 3, of its own coordinates and alpha, 51 / 255 = 0.2.
    let overlay = &meshes.overlay;
    assert_eq!(overlay.positions, ground.positions[12..]);
    assert_eq!(overlay.normals, ground.normals[12..]);
    assert_eq!(overlay.uvs[0], Vec2::splat(0.5));
    assert_eq!(overlay.uvs[2], Vec2::new(0.75, 1.0));
    assert_eq!(
        overlay
            .colors
            .iter()
            .map(|color| color.w)
            .collect::<Vec<_>>(),
        [0.0, 1.0, 0.2, 0.0]
    );
    assert_eq!(overlay.indices, [0, 2, 1, 0, 3, 2]);
}
