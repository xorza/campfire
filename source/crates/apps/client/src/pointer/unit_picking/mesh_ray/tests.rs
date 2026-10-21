use bevy::asset::RenderAssetUsages;
use bevy::mesh::{MeshBuilder, Meshable};
use bevy::shape::Cuboid;

use super::*;

#[test]
fn a_line_meets_a_box_on_its_nearest_face_and_a_mesh_on_its_nearest_triangle() {
    // A box of half sides 0.5, 1 and 0.5 round (0, 1, 0): a line at 1 m from z = 10 along −z
    // meets its face z = 0.5, 9.5 along; at 2.5 m it passes over it; from inside it meets it where
    // it starts; along +z, never.
    let (center, half) = (Vec3::Y, Vec3::new(0.5, 1.0, 0.5));
    let entry = |from: Vec3, way| box_entry(from, way, center, half);
    assert_eq!(entry(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z), Some(9.5));
    assert_eq!(entry(Vec3::new(0.0, 2.5, 10.0), Vec3::NEG_Z), None);
    assert_eq!(entry(Vec3::Y, Vec3::NEG_Z), Some(0.0));
    assert_eq!(entry(Vec3::new(0.0, 1.0, 10.0), Vec3::Z), None);

    // The cube of side 2 round the origin, as twelve triangles: a line from z = 10 along −z
    // meets its face z = 1, 9 along, or 4.5 along a way twice as long; 0.1 m past its edge at
    // x = 1 it misses; from inside it meets the face ahead, 1 along; along +z, never.
    let cube = Cuboid::new(2.0, 2.0, 2.0).mesh().build();
    let entry = |from: Vec3, way| triangles_entry(from, way, &cube);
    assert_eq!(entry(Vec3::new(0.9, 0.0, 10.0), Vec3::NEG_Z), Some(9.0));
    assert_eq!(
        entry(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z * 2.0),
        Some(4.5)
    );
    assert_eq!(entry(Vec3::new(1.1, 0.0, 10.0), Vec3::NEG_Z), None);
    assert_eq!(entry(Vec3::ZERO, Vec3::NEG_Z), Some(1.0));
    assert_eq!(entry(Vec3::new(0.0, 0.0, 10.0), Vec3::Z), None);

    // One triangle of no indices, (0, 0, 0), (1, 0, 0), (0, 1, 0), met from behind as from
    // before; a line in its plane grazes it, and a list of lines holds no triangle.
    let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let mesh = |topology| {
        Mesh::new(topology, RenderAssetUsages::all())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
    };
    let triangle = mesh(PrimitiveTopology::TriangleList);
    let at = Vec3::new(0.25, 0.25, 0.0);
    assert_eq!(
        triangles_entry(at + Vec3::Z * 3.0, Vec3::NEG_Z, &triangle),
        Some(3.0)
    );
    assert_eq!(
        triangles_entry(at - Vec3::Z * 3.0, Vec3::Z, &triangle),
        Some(3.0)
    );
    assert_eq!(
        triangles_entry(Vec3::new(-1.0, 0.25, 0.0), Vec3::X, &triangle),
        None
    );
    let lines = mesh(PrimitiveTopology::LineList);
    assert_eq!(
        triangles_entry(at + Vec3::Z * 3.0, Vec3::NEG_Z, &lines),
        None
    );
}
