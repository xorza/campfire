//! Fixed-point numbers, 3D vectors, exact vectors of the ground plane, trig, exact integer
//! roots, directions, the counter-based RNG, and 256-bit lanes, each operation one 128-bit step
//! rounded once.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod direction;
mod flat;
mod num;
mod rng;
mod root;
mod rounding;
mod simd;
mod vec3;
mod wide_division;

pub use crate::direction::Direction;
pub use crate::flat::Flat;
pub use crate::num::error::{IntToNumError, ParseNumError};
pub use crate::num::{Num, SinCos};
pub use crate::rng::Rng;
pub use crate::rng::rng_stream::RngStream;
pub use crate::root::{FloorRoot, RoundedRoot};
pub use crate::rounding::Rounding;
pub use crate::simd::i64x4::I64x4;
pub use crate::simd::mask64x4::Mask64x4;
pub use crate::simd::u64x4::U64x4;
pub use crate::vec3::Vec3;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::rng::split_mix64::SplitMix64;
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::{direction, num, rng, root, rounding, vec3};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        num::bench::num(c);
        root::bench::root(c);
        rng::bench::rng(c);
        vec3::bench::vec3(c);
        direction::bench::direction(c);
        rounding::bench::rounding(c);
    }
}
