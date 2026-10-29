//! The plane's geometry as the game's `Vector2` and `LineSegClass` compute it, in `f32`.

use bevy_math::Vec2;

/// `vector` turned by `angle`, counter-clockwise (`Vector2::Rotate`, `Vector3::Rotate_Z`).
pub(super) fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = libm::sincosf(angle);
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

/// `point` turned by `angle` about `center` (`rotateAbout`).
pub(super) fn rotate_about(point: Vec2, center: Vec2, angle: f32) -> Vec2 {
    let offset = point - center;
    point + (rotate(offset, angle) - offset)
}

/// Where the lines through `one` and `two` cross; none for parallel lines, or one of a single
/// point (`LineSegClass::Find_Intersection`).
pub(super) fn intersection(one: [Vec2; 2], two: [Vec2; 2]) -> Option<Vec2> {
    let [first, second] = [one, two].map(|[from, to]| (to - from).normalize_or_zero());
    let cross = first.perp_dot(second);
    if cross == 0.0 {
        return None;
    }
    let along = (two[0] - one[0]).perp_dot(second) * cross / (cross * cross);
    Some(one[0] + first * along)
}

/// The way `two` turns from `one`, by the sign of their cross product (`xpSign`).
pub(super) fn turn(one: Vec2, two: Vec2) -> TurnSign {
    let cross = one.perp_dot(two);
    if cross < 0.0 {
        TurnSign::Right
    } else if cross > 0.0 {
        TurnSign::Left
    } else {
        TurnSign::Along
    }
}

/// Which way a vector turns from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnSign {
    Left,
    Along,
    Right,
}
