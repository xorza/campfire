use std::arch::asm;

use crate::wide_division::WideDivision;

/// A divisor and a quotient that fit 64 bits take one `div` instruction inline, which divides
/// 128 bits by 64; others Rust's `u128` division, a library call (07-determinism-core.md, One div
/// instruction for a wide division on x86-64).
#[expect(clippy::cast_possible_truncation, reason = "the divisor fits 64 bits")]
#[inline]
pub(super) fn divide(numerator: u128, divisor: u128) -> WideDivision {
    debug_assert!(divisor > 0);
    let high = (numerator >> 64) as u64;
    if divisor >> 64 == 0 && u128::from(high) < divisor {
        let (quotient, rest): (u64, u64);
        // SAFETY: `div` faults on a divisor of 0, which `WideDivision::of` refuses, and on a
        // quotient past 64 bits, which a high half below the divisor rules out. It reads and
        // writes registers alone.
        unsafe {
            asm!(
                "div {divisor}",
                divisor = in(reg) divisor as u64,
                inout("rax") numerator as u64 => quotient,
                inout("rdx") high => rest,
                options(pure, nomem, nostack),
            );
        }
        WideDivision {
            quotient: u128::from(quotient),
            rest: u128::from(rest),
        }
    } else {
        WideDivision {
            quotient: numerator / divisor,
            rest: numerator % divisor,
        }
    }
}
