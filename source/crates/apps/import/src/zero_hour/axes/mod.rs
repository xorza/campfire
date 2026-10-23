//! The game's frames on the engine's: W3D's and a map's `z` up, the engine's and glTF's `y`
//! up, and the game's north the engine's `−z` (09-zero-hour.md, Axes stay right-handed).

use campfire_math::{Num, Vec3};

/// A point or a direction of W3D's frame, `z` up, on glTF's axes, `y` up: `(x, y, z)` as
/// `(x, z, −y)`, a rotation, so handedness and winding hold.
pub(crate) const fn y_up(vector: [f32; 3]) -> [f32; 3] {
    [vector[0], vector[2], -vector[1]]
}

/// A rotation of W3D's frame on glTF's axes: its axis turned as a vector is, its angle kept.
pub(crate) const fn y_up_rotation(rotation: [f32; 4]) -> [f32; 4] {
    [rotation[0], rotation[2], -rotation[1], rotation[3]]
}

/// A point of the game's ground, `x` east and `y` north, on the engine's ground plane: `(x, −y)`
/// as its `x` and `z`, as `y_up` turns a point, so a turn from `x` towards `y` keeps its sign,
/// counter-clockwise seen from above. `None` past the world's bound.
pub(crate) fn ground([x, y]: [Num; 2]) -> Option<[Num; 2]> {
    let ground = [x, y.checked_neg()?];
    let bound = Vec3::BOUND.to_bits().unsigned_abs();
    ground
        .iter()
        .all(|value| value.to_bits().unsigned_abs() <= bound)
        .then_some(ground)
}

pub(crate) mod cell_frame;

#[cfg(test)]
mod tests;
