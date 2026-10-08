use campfire_math::Num;
use campfire_sim::Position;

/// A point or a vector of the ground plane, `[x, z]`, in a `Num`'s bits or, at twice the scale,
/// in halves of a bit, where a grid cell's center is whole: exact for sums and products of
/// coordinates within twice the world's bound.
pub(crate) type Flat = [i128; 2];

/// Coordinates in halves of a bit.
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
        [Halves::of(at.x), Halves::of(at.z)]
    }
}
