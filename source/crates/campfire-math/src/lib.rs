//! Fixed-point numbers, 3D vectors, exact vectors of the ground plane, trig, exact integer
//! roots, exact 256-bit products and their sums, the counter-based RNG, and 256-bit lanes.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod flat;
mod floor_root;
mod num;
mod product_sum;
mod rng;
mod simd;
mod u256;
mod vec3;

pub use crate::flat::Flat;
pub use crate::floor_root::FloorRoot;
pub use crate::num::error::ParseNumError;
pub use crate::num::{Num, SinCos};
pub use crate::product_sum::ProductSum;
pub use crate::rng::Rng;
pub use crate::rng::rng_opener::RngOpener;
pub use crate::rng::rng_source::RngSource;
pub use crate::rng::rng_stream::RngStream;
pub use crate::simd::i64x4::I64x4;
pub use crate::simd::mask64x4::Mask64x4;
pub use crate::simd::u64x4::U64x4;
pub use crate::u256::U256;
pub use crate::vec3::Vec3;

#[cfg(any(test, feature = "internals"))]
pub mod internals {
    pub use crate::rng::split_mix64::SplitMix64;
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    use crate::{floor_root, num, product_sum, rng, u256, vec3};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        num::bench::num(c);
        floor_root::bench::root(c);
        rng::bench::rng(c);
        vec3::bench::vec3(c);
        u256::bench::u256(c);
        product_sum::bench::product_sum(c);
    }
}
