//! Fixed-point numbers, 3D vectors, trig, exact 256-bit products and their sums, and the
//! counter-based RNG.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod num;
mod product_sum;
mod rng;
mod u256;
mod vec3;

pub use num::Num;
pub use num::error::ParseNumError;
pub use product_sum::ProductSum;
pub use rng::Rng;
pub use rng::rng_source::RngSource;
pub use rng::rng_stream::RngStream;
pub use u256::U256;
pub use vec3::Vec3;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::num::bench::num;
    pub use crate::rng::bench::rng;
    pub use crate::vec3::bench::vec3;
}
