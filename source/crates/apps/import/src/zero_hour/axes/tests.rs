use super::*;

#[test]
fn w3d_s_up_is_gltf_s_y_and_its_north_gltf_s_negative_z() {
    assert_eq!(y_up([1.0, 2.0, 3.0]), [1.0, 3.0, -2.0]);
    assert_eq!(y_up_rotation([0.1, 0.2, 0.3, 0.9]), [0.1, 0.3, -0.2, 0.9]);
}
