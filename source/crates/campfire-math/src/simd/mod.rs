//! Lanes of 256 bits: four 64-bit lanes, the same on x86-64-v3 and armv8-a. Each op is one
//! operation lane by lane, written so that LLVM lowers it to one vector instruction or a short
//! exact sequence on both. An op wraps; its doc states the domain inside which it cannot.

pub(crate) mod f64x4;
pub(crate) mod i64x4;
pub(crate) mod mask64x4;
pub(crate) mod u64x4;
