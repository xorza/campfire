//! The game's logic frames, 30 a second, in which its data counts each rate and each time
//! (`LOGICFRAMES_PER_SECOND`; 09-zero-hour.md, Thirty frames a second, the original's times). Each
//! constant is the `f32` the game computes it as.

/// The frames of a second, `LOGICFRAMES_PER_SECOND`.
pub(crate) const PER_SECOND: f32 = 30.0;

/// The seconds of a frame, `SECONDS_PER_LOGICFRAME_REAL`.
pub(crate) const SECONDS: f32 = 1.0 / PER_SECOND;

/// The frames of a millisecond, `LOGICFRAMES_PER_MSEC_REAL`.
pub(crate) const PER_MS: f32 = PER_SECOND / 1000.0;
