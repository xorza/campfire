//! The game's time: its 30 logic frames a second, by which the import writes every time it takes
//! of the game (09-zero-hour.md, Thirty frames a second, the original's times).

use crate::zero_hour::error::TimeError;
use crate::zero_hour::format::game_frames;

/// The milliseconds of the frames the game turns `ms`, a whole time, into, as `of_duration`
/// turns a real one (`parseShotDelay`); `Negative` for a time below 0.
pub(crate) fn of_ms(ms: i32) -> Result<u64, TimeError> {
    of_duration(ms as f32)
}

/// The milliseconds of the frames the game turns `ms` into: its `ceil(ms × 0.03)` in its `f32`
/// (`INI::parseDurationUnsignedInt`); `Negative` for a time below 0, and `PastInt` for one not
/// finite or of more frames than the game's `Int` holds.
pub(crate) fn of_duration(ms: f32) -> Result<u64, TimeError> {
    /// 2³¹, the least count of frames past an `Int`.
    const PAST_INT: f64 = 2_147_483_648.0;
    if ms < 0.0 {
        return Err(TimeError::Negative);
    }
    let frames = (ms * game_frames::PER_MS).ceil();
    // Not a number and infinity are out of the range too.
    if !(0.0..PAST_INT).contains(&f64::from(frames)) {
        return Err(TimeError::PastInt);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole count of frames at least 0 and below 2³¹"
    )]
    let frames = frames as u64;
    Ok(frames_ms(frames))
}

/// The milliseconds of a build time of `seconds`, `BuildTime × 30` truncated, as
/// `calcTimeToBuild` counts its frames; `PastInt` for a time below 0, not finite, or of more
/// frames than the game's `Int` holds, 2³¹ and past, which its cast does not define.
pub(crate) fn of_build_seconds(seconds: f32) -> Result<u64, TimeError> {
    /// 2³¹, the least count of frames past an `Int`.
    const PAST_INT: f64 = 2_147_483_648.0;
    let frames = seconds * game_frames::PER_SECOND;
    // Not a number and either infinity are out of the range too.
    if !(0.0..PAST_INT).contains(&f64::from(frames)) {
        return Err(TimeError::PastInt);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the game truncates the frames of a build to an `Int`, toward 0"
    )]
    let frames = frames as u64;
    Ok(frames_ms(frames))
}

/// The milliseconds the engine turns into `frames`: `⌊frames × 100 / 3⌋`, which its rounding up
/// turns back into the same frames.
pub(crate) const fn frames_ms(frames: u64) -> u64 {
    frames * 100 / 3
}

#[cfg(test)]
mod tests;
