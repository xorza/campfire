use bevy::math::Vec3;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};

/// How far along the line through `from` along `way` it first meets the box of `half` sides
/// round `center`, in multiples of `way`; none when it misses the box or meets it only behind
/// `from`, and 0 from inside it.
pub(super) fn box_entry(from: Vec3, way: Vec3, center: Vec3, half: Vec3) -> Option<f32> {
    let (mut near, mut far) = (0.0_f32, f32::INFINITY);
    for axis in 0..3 {
        let (from, step) = (from[axis] - center[axis], way[axis]);
        if step == 0.0 {
            if from.abs() > half[axis] {
                return None;
            }
            continue;
        }
        let [a, b] = [(-half[axis] - from) / step, (half[axis] - from) / step];
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far).then_some(near)
}

/// How far along the line through `from` along `way`, in `mesh`'s own frame, it first meets a
/// triangle of `mesh`, from either side, in multiples of `way`; none when it meets none ahead of
/// `from`, or for a mesh that is no list of triangles of three-float positions.
pub(super) fn triangles_entry(from: Vec3, way: Vec3, mesh: &Mesh) -> Option<f32> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    match mesh.indices() {
        Some(Indices::U16(indices)) => nearest(
            from,
            way,
            positions,
            (indices.as_chunks::<3>().0.iter()).map(|at| at.map(usize::from)),
        ),
        Some(Indices::U32(indices)) => nearest(
            from,
            way,
            positions,
            (indices.as_chunks::<3>().0.iter())
                .map(|at| at.map(|index| usize::try_from(index).expect("an index fits usize"))),
        ),
        None => nearest(
            from,
            way,
            positions,
            (0..positions.len() / 3).map(|at| [3 * at, 3 * at + 1, 3 * at + 2]),
        ),
    }
}

/// How far along the line through `from` along `way` it first meets a triangle of `triangles`,
/// each three indices into `positions`, in multiples of `way`; a triangle of an index past
/// `positions` is none.
fn nearest(
    from: Vec3,
    way: Vec3,
    positions: &[[f32; 3]],
    triangles: impl Iterator<Item = [usize; 3]>,
) -> Option<f32> {
    let corner = |index: usize| positions.get(index).copied().map(Vec3::from);
    triangles
        .filter_map(|[a, b, c]| triangle_entry(from, way, [corner(a)?, corner(b)?, corner(c)?]))
        .min_by(f32::total_cmp)
}

/// How far along the line through `from` along `way` it meets the triangle `corners`, from
/// either side, in multiples of `way`, by Möller and Trumbore's test; none when it misses it,
/// meets it behind `from`, or lies in its plane, where a ray grazes no surface.
fn triangle_entry(from: Vec3, way: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = way.cross(ac);
    let determinant = ab.dot(p);
    if determinant == 0.0 {
        return None;
    }
    let to_from = from - a;
    let u = to_from.dot(p) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = to_from.cross(ab);
    let v = way.dot(q) / determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(q) / determinant;
    (t >= 0.0).then_some(t)
}

#[cfg(test)]
mod tests;
