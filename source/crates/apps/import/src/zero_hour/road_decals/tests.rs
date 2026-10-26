use bevy_math::UVec2;

use super::*;

/// A mesh of `points`, each with its coordinates, and `triangles`.
fn mesh(corners: &[([f32; 3], [f32; 2])], triangles: &[[u32; 3]]) -> RoadMesh {
    let mut mesh = RoadMesh::default();
    mesh.points = corners
        .iter()
        .map(|(point, _)| Vec3::from(*point))
        .collect();
    mesh.uvs = corners.iter().map(|(_, uv)| Vec2::from(*uv)).collect();
    mesh.triangles = triangles.to_vec();
    mesh
}

/// The decals' vertices as their points on the engine's axes and their coordinates.
fn vertices(decals: &TerrainDecals) -> Vec<([f32; 3], [f32; 2])> {
    decals
        .vertices
        .iter()
        .map(|vertex| (vertex.position.to_array(), vertex.uv.to_array()))
        .collect()
}

#[test]
fn a_triangle_is_split_where_its_coordinates_cross_a_repeat_each_part_on_the_copy() {
    // A quad 20 long and 10 across, `u` from 0.5 to 1.5 along it and `v` from 0.75 to 0.25
    // across, its texture's copy at (100, 200) of 64 × 32 texels.
    let quad = mesh(
        &[
            ([0.0, 0.0, 1.0], [0.5, 0.75]),
            ([0.0, 10.0, 1.0], [0.5, 0.25]),
            ([20.0, 0.0, 3.0], [1.5, 0.75]),
            ([20.0, 10.0, 3.0], [1.5, 0.25]),
        ],
        &[[1, 0, 2], [1, 2, 3]],
    );
    let place = TexturePlace {
        corner: UVec2::new(100, 200),
        size: UVec2::new(64, 32),
    };
    let mut decals = RoadDecals::default();
    decals.add(&quad, place);
    let decals = decals.into_decals();
    // Each triangle crosses `u` = 1 halfway along: its part in the first repeat and its part in
    // the second, each corner at its share of its repeat over the copy, `u` 0.5 at 100 + 32 and
    // 1 at 164 in the first, 100 in the second, `v` 0.25 at 200 + 8. A point's `y` is the
    // engine's `−z`, and its height the engine's `y`.
    assert_eq!(
        vertices(&decals),
        [
            ([0.0, 1.0, -10.0], [132.0, 208.0]),
            ([0.0, 1.0, 0.0], [132.0, 224.0]),
            ([10.0, 2.0, 0.0], [164.0, 224.0]),
            ([10.0, 2.0, -5.0], [164.0, 216.0]),
            ([10.0, 2.0, 0.0], [100.0, 224.0]),
            ([20.0, 3.0, 0.0], [132.0, 224.0]),
            ([10.0, 2.0, -5.0], [100.0, 216.0]),
            ([0.0, 1.0, -10.0], [132.0, 208.0]),
            ([10.0, 2.0, -5.0], [164.0, 216.0]),
            ([10.0, 2.0, -10.0], [164.0, 208.0]),
            ([10.0, 2.0, -5.0], [100.0, 216.0]),
            ([20.0, 3.0, 0.0], [132.0, 224.0]),
            ([20.0, 3.0, -10.0], [132.0, 208.0]),
            ([10.0, 2.0, -10.0], [100.0, 208.0]),
        ]
    );
    assert_eq!(
        decals.triangles,
        [
            [0, 1, 2],
            [0, 2, 3],
            [4, 5, 6],
            [7, 8, 9],
            [10, 11, 12],
            [10, 12, 13]
        ]
    );
    // Seen from above, the engine's `−y`, each turns as the game's did.
    for triangle in &decals.triangles {
        let [a, b, c] = triangle.map(|at| decals.vertices[at as usize].position);
        assert!((b - a).cross(c - a).y > 0.0, "{triangle:?} faces up");
    }

    // A triangle within one whole repeat, its `u` from −1 to −0.5, is the copy's left half,
    // whole; its corner on the repeat's edge, `u` −1, is no crossing.
    let inside = mesh(
        &[
            ([0.0, 10.0, 0.0], [-1.0, 0.0]),
            ([0.0, 0.0, 0.0], [-1.0, 1.0]),
            ([10.0, 0.0, 0.0], [-0.5, 1.0]),
        ],
        &[[0, 1, 2]],
    );
    let mut decals = RoadDecals::default();
    decals.add(&inside, place);
    let decals = decals.into_decals();
    let uvs: Vec<[f32; 2]> = vertices(&decals).into_iter().map(|(_, uv)| uv).collect();
    assert_eq!(uvs, [[100.0, 200.0], [100.0, 232.0], [132.0, 232.0]]);
    assert_eq!(decals.triangles, [[0, 1, 2]]);
}
