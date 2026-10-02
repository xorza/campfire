//! Fixed-point numbers, 3D vectors, trig, the counter-based RNG, and the player slot and the
//! ticks that the session log and the sim share.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes32;
mod num;
mod player_slot;
mod rng;
mod tick;
mod u256;
mod vec3;

pub use bytes32::Bytes32;
pub use bytes32::error::NotHex;
pub use num::Num;
pub use num::error::ParseNumError;
pub use player_slot::PlayerSlot;
pub use rng::Rng;
pub use rng::rng_source::RngSource;
pub use rng::rng_stream::RngStream;
pub use rng::segment_seed::SegmentSeed;
pub use tick::{Tick, Ticks};
pub use u256::U256;
pub use vec3::Vec3;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::num::bench::num;
    pub use crate::rng::bench::rng;
    pub use crate::vec3::bench::vec3;
}
