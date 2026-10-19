//! Coordinates in halves of a bit, the scale at which a grid cell's center is whole. Within
//! twice the world's bound, they and their sums fit a `Flat`'s `i64`s, and their products its
//! `i128` ones.

use campfire_math::{Flat, Num, Vec3};

/// `value` in halves of a bit, exactly.
pub(crate) const fn of(value: Num) -> i128 {
    2 * value.to_bits() as i128
}

/// Where `position` stands on the ground plane, in halves of a bit.
pub(crate) const fn ground(position: Vec3) -> Flat {
    Flat::new(of(position.x), of(position.z))
}
