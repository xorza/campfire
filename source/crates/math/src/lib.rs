//! Fixed-point numbers, 3D vectors, trig, the counter-based RNG, and the player slot that the
//! session log and the sim share.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

pub mod hex;
mod num;
mod player_slot;
mod rng;
mod u256;
mod vec3;

pub use hex::error::NotHex;
pub use num::error::ParseNumError;
pub use num::{Num, SinCos};
pub use player_slot::PlayerSlot;
pub use rng::Rng;
pub use rng::rng_source::{RngSource, SegmentSeed};
pub use u256::U256;
pub use vec3::Vec3;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::num::bench::num;
    pub use crate::rng::bench::rng;
    pub use crate::vec3::bench::vec3;
}
