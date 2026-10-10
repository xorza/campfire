use campfire_math::{Flat, Num};
use campfire_sim::Position;

/// Coordinates in halves of a bit, the scale at which a grid cell's center is whole. Within
/// twice the world's bound, their sums and products fit a `Flat`'s `i128`s.
#[derive(Debug)]
pub(crate) struct Halves;

impl Halves {
    /// `value` in halves of a bit, exactly.
    pub(crate) const fn of(value: Num) -> i128 {
        2 * value.to_bits() as i128
    }

    /// Where `pos` stands on the ground plane, in halves of a bit.
    pub(crate) const fn ground(pos: Position) -> Flat {
        let at = pos.get();
        Flat::new(Halves::of(at.x), Halves::of(at.z))
    }
}
