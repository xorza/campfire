//! The game's time: its 30 logic frames a second, by which the import writes every time it takes
//! of the game (09-zero-hour.md, Thirty frames a second, the original's times).

use crate::zero_hour::error::TimeError;

/// The game's logic frames a second, `LOGICFRAMES_PER_SECOND`.
pub(crate) const FRAMES_PER_SECOND: f32 = 30.0;

/// The game's logic frames a millisecond, `LOGICFRAMES_PER_MSEC_REAL`, in the game's `f32`.
const FRAMES_PER_MS: f32 = FRAMES_PER_SECOND / 1000.0;

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
    let frames = (ms * FRAMES_PER_MS).ceil();
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
    let frames = seconds * FRAMES_PER_SECOND;
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
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_as_the_milliseconds_of_the_games_frames() {
        // 1 ms is ⌈0.03⌉ = 1 frame, 33 ms; 2,000 ms are 60 frames, 2,000; a time below 0 is
        // refused.
        assert_eq!(of_ms(1), Ok(33));
        assert_eq!(of_ms(2000), Ok(2000));
        assert_eq!(of_ms(-1), Err(TimeError::Negative));
        // A real time rounds the same: 1 ms and 0.5 ms are 1 frame, 33 ms; 1,001 ms are
        // ⌈30.03⌉ = 31 frames, 1,033 ms. Below 0 is refused; a time not finite, or of 2³¹ frames
        // and past, holds no `Int` of frames.
        assert_eq!([of_duration(1.0), of_duration(0.5)], [Ok(33), Ok(33)]);
        assert_eq!(of_duration(1001.0), Ok(1033));
        assert_eq!(of_duration(-0.5), Err(TimeError::Negative));
        for past in [f32::NAN, f32::INFINITY, 1e11] {
            assert_eq!(of_duration(past), Err(TimeError::PastInt), "{past}");
        }
    }

    #[test]
    #[expect(
        clippy::lossy_float_literal,
        reason = "71,582,784 and 71,582,792 are multiples of 8, which an f32 of 2²⁶ to 2²⁷ holds exactly"
    )]
    fn a_build_time_is_its_frames_in_milliseconds_within_the_games_int() {
        // 5 s is 150 frames, ⌊150 × 100 / 3⌋ = 5,000 ms; 0.1 s is 3 frames, 100 ms. 71,582,784 s
        // is 2,147,483,520 frames, the most an `f32` holds below 2³¹, ⌊2,147,483,520 × 100 / 3⌋
        // = 71,582,784,000 ms; 71,582,792 s, the next time an `f32` holds, is 2³¹ frames, past
        // the game's `Int`, and gives none, as what is not a time at all does.
        let windup = |seconds| of_build_seconds(seconds).ok();
        assert_eq!(windup(5.0), Some(5000));
        assert_eq!(windup(0.1), Some(100));
        assert_eq!(windup(0.0), Some(0));
        assert_eq!(windup(71_582_784.0), Some(71_582_784_000));
        for refused in [71_582_792.0, f32::MAX, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(windup(refused), None, "{refused}");
        }
    }
}
