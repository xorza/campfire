use crate::num::Num;
use crate::vec3::step_moves::StepMoves;

/// Each move by its division: a product below 2⁶⁴ over the distance is one `div`, which a float
/// estimate and its correction measured 26 % slower than, on a Ryzen 7 6800U.
#[inline]
pub(super) fn moves(moves: StepMoves, offsets: [Num; 3]) -> [Num; 3] {
    offsets.map(|offset| moves.divided(offset))
}
