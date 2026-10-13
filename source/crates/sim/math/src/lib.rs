//! Fixed-point numbers, 3D vectors, exact vectors of the ground plane, trig, exact integer
//! roots, exact 256-bit products and their sums, exact ratios, the counter-based RNG, and 256-bit
//! lanes.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod ceil_root;
mod flat;
mod floor_root;
mod i256;
mod num;
mod product_sum;
mod ratio;
mod rng;
mod rounding;
mod simd;
mod u256;
mod u384;
mod vec3;
mod wide_division;

pub use crate::ceil_root::CeilRoot;
pub use crate::flat::Flat;
pub use crate::floor_root::FloorRoot;
pub use crate::i256::I256;
pub use crate::num::error::{IntToNumError, ParseNumError};
pub use crate::num::{Num, SinCos};
pub use crate::product_sum::ProductSum;
pub use crate::ratio::{CrossProduct, Ratio, RatioPart};
pub use crate::rng::Rng;
pub use crate::rng::rng_stream::RngStream;
pub use crate::rounding::Rounding;
pub use crate::simd::i64x4::I64x4;
pub use crate::simd::mask64x4::Mask64x4;
pub use crate::simd::u64x4::U64x4;
pub use crate::u256::U256;
pub use crate::u384::U384;
pub use crate::vec3::Vec3;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::rng::split_mix64::SplitMix64;
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::{
        ceil_root, floor_root, i256, num, product_sum, ratio, rng, rounding, u256, u384, vec3,
    };

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        num::bench::num(c);
        floor_root::bench::floor_root(c);
        ceil_root::bench::ceil_root(c);
        rng::bench::rng(c);
        vec3::bench::vec3(c);
        u256::bench::u256(c);
        i256::bench::i256(c);
        u384::bench::u384(c);
        ratio::bench::ratio(c);
        product_sum::bench::product_sum(c);
        rounding::bench::rounding(c);
    }
}
