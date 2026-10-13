use campfire_math::{Flat, Num};
use campfire_sim::Position;

/// Coordinates in halves of a bit, the scale at which a grid cell's center is whole. Within
/// twice the world's bound, they and their sums fit a `Flat`'s `i64`s, and their products its
/// `i128` ones.
#[derive(Debug)]
pub struct Halves;

impl Halves {
    /// `value` in halves of a bit, exactly.
    pub const fn of(value: Num) -> i128 {
        2 * value.to_bits() as i128
    }

    /// Where `pos` stands on the ground plane, in halves of a bit.
    pub const fn ground(pos: Position) -> Flat {
        let point = pos.get();
        Flat::new(Halves::of(point.x), Halves::of(point.z))
    }
}
