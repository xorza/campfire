use super::*;
use crate::zero_hour::axes::cell_frame::CellFrame;

#[test]
fn w3d_s_up_is_gltf_s_y_and_its_north_gltf_s_negative_z() {
    assert_eq!(y_up([1.0, 2.0, 3.0]), [1.0, 3.0, -2.0]);
    assert_eq!(y_up_rotation([0.1, 0.2, 0.3, 0.9]), [0.1, 0.3, -0.2, 0.9]);
    // The game's north, its `y`, is the engine's `−z`, within the world's bound alone.
    assert_eq!(
        ground([Num::int(30), Num::int(20)]),
        Some([Num::int(30), Num::int(-20)])
    );
    let bound = Vec3::BOUND;
    assert_eq!(ground([bound, -bound]), Some([bound, bound]));
    assert_eq!(ground([bound + Num::from_bits(1), Num::ZERO]), None);
    assert_eq!(ground([Num::ZERO, Num::MIN]), None);
    // A map's sample `[i, j]` of a border of 2 and 10 m cells is the game's point
    // `((i − 2) · 10, (j − 2) · 10)`: `[2, 2]` the origin, `[0, 5]` at x −20 and z −30.
    let frame = CellFrame {
        border: 2,
        cell: Num::int(10),
    };
    assert_eq!(frame.sample([2, 2]), Some([Num::ZERO; 2]));
    assert_eq!(frame.sample([0, 5]), Some([Num::int(-20), Num::int(-30)]));
}
