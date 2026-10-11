//! The moves of `Vec3::checked_step_toward`, exact on every platform, each by its fastest way:
//! x86-64 divides each by the distance natively; ARM takes a float estimate of each from one
//! ratio, and corrects it exactly.

#[cfg(any(test, not(target_arch = "x86_64")))]
mod aarch64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

use crate::num::Num;
#[cfg(not(target_arch = "x86_64"))]
use crate::vec3::step_moves::aarch64 as platform;
#[cfg(target_arch = "x86_64")]
use crate::vec3::step_moves::x86_64 as platform;

/// A step of `step` bits toward a target `distance` bits away, at least the step.
#[derive(Debug, Clone, Copy)]
pub(super) struct StepMoves {
    step: i64,
    distance: i64,
}

impl StepMoves {
    pub(super) const fn new(step: i64, distance: i64) -> StepMoves {
        debug_assert!(0 <= step && step <= distance);
        StepMoves { step, distance }
    }

    /// Each offset's move, `offset · step / distance`, rounded to nearest, ties to even: at most
    /// the offset, for offsets within the distance.
    #[inline]
    pub(super) fn of(self, offsets: [Num; 3]) -> [Num; 3] {
        platform::moves(self, offsets)
    }

    /// One move by the exact division.
    fn divided(self, offset: Num) -> Num {
        Num::from_raw_ratio(
            i128::from(offset.to_bits()) * i128::from(self.step),
            i128::from(self.distance),
        )
        .expect("a move is at most its offset")
    }
}

#[cfg(test)]
mod tests;
