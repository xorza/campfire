/// A point or a direction of W3D's frame, `z` up, on glTF's axes, `y` up: `(x, y, z)` as
/// `(x, z, −y)`, a rotation, so handedness and winding hold.
pub(crate) const fn y_up(vector: [f32; 3]) -> [f32; 3] {
    [vector[0], vector[2], -vector[1]]
}

/// A rotation of W3D's frame on glTF's axes: its axis turned as a vector is, its angle kept.
pub(crate) const fn y_up_rotation(rotation: [f32; 4]) -> [f32; 4] {
    [rotation[0], rotation[2], -rotation[1], rotation[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w3d_s_up_is_gltf_s_y_and_its_north_gltf_s_negative_z() {
        assert_eq!(y_up([1.0, 2.0, 3.0]), [1.0, 3.0, -2.0]);
        assert_eq!(y_up_rotation([0.1, 0.2, 0.3, 0.9]), [0.1, 0.3, -0.2, 0.9]);
    }
}
