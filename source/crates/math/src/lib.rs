//! Fixed-point numbers, 3D vectors, trig and the counter-based RNG.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod num;

pub use num::error::ParseNumError;
pub use num::{Num, SinCos};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::num::bench::num;
}
