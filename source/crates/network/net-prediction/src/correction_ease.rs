use bevy_curve::{Curve, EaseFunction};
use bevy_reflect::Reflect;
use core::fmt::Debug;

/// A curve that gives a correction error up over time.
///
/// Every error is a recorded jump, and this is how it is given up. The curves
/// come in two shapes:
///
/// * **Unbounded**: [`Exponential`](Self::Exponential) has no end of its own —
///   `decay_ratio` of the error is left after each `decay_period_secs`, so it only
///   ever approaches zero. This is the shape for reconciliation, where the aim is
///   to catch up with the server; it is what `CorrectionPolicy` uses.
/// * **Bounded**: the rest are normalised over a window, reaching zero at its
///   end. This is the shape for a timeline switch, where the transition is meant
///   to be seen and to finish.
///
/// A bounded curve gets its length from the window it runs over
/// (`SwitchBlend`'s duration) rather than from the variant, so there is one
/// place to set it. An exponential carries its own period, because that period
/// is what defines its shape; a window still bounds it, and the error converges
/// when the window ends.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub enum CorrectionEase {
    /// Constant speed.
    Linear,
    /// Slow start and slow finish.
    ///
    /// A flat start hides the velocity step when the entity was already moving,
    /// but it holds almost the whole correction for the first frames — see
    /// [`EaseOutCubic`](Self::EaseOutCubic) — which reads as a stop on a blend
    /// that starts from a large gap.
    Smoothstep,
    /// Fast start, slow finish.
    ///
    /// An error is given up from the value on screen towards the destination, so
    /// a curve that starts fast releases a visible amount on the first frame and
    /// settles gradually. A flat-starting curve instead keeps almost the whole
    /// gap for those frames — a frame of a one second `Smoothstep` window
    /// releases under 1% of it — which reads as the entity stopping and then
    /// lurching.
    EaseOutCubic,
    /// Slow start, fast middle, slow finish.
    EaseInOutCubic,
    /// `decay_ratio` of the error remains after each `decay_period_secs` seconds.
    ///
    /// Unlike the curves above this one has no end of its own, so it is the
    /// window it runs in that ends it. Its shape is
    /// its parameters: it gives up the same fraction every frame, rather than
    /// following a curve.
    ///
    /// This is [`Self::default`], and the shape is the one a blend wants: the
    /// gap is released at a steady rate from the first frame instead of being
    /// held while the curve warms up.
    Exponential {
        /// Fraction of the error left after one `decay_period_secs`.
        decay_ratio: f32,
        /// Time for the error to fall to `decay_ratio` of its value.
        decay_period_secs: f32,
    },
}

impl Default for CorrectionEase {
    /// The tuned blend shape: half the remaining error every 200 ms.
    ///
    /// Half the error goes in the first 200 ms, three quarters in 400 ms, and so
    /// on, so a blend releases a little under 6% of its gap per 60 Hz frame from
    /// the very first one. That is enough to keep a moving entity moving through
    /// a switch, where a flat-starting curve would hold nearly all of the gap.
    ///
    /// A struct variant cannot carry `#[default]`, so this is written out rather
    /// than derived — which keeps it the single definition of the default, used
    /// by both `CorrectionPolicy` and
    /// `TimelineSwitchConfig`.
    fn default() -> Self {
        Self::Exponential {
            decay_ratio: Self::DEFAULT_DECAY_RATIO,
            decay_period_secs: Self::DEFAULT_DECAY_PERIOD_SECS,
        }
    }
}

impl CorrectionEase {
    /// The default exponential's fraction of the error left after each period.
    pub const DEFAULT_DECAY_RATIO: f32 = 0.5;
    /// The default exponential's period, in seconds.
    pub const DEFAULT_DECAY_PERIOD_SECS: f32 = 0.2;

    /// How long an exponential takes to keep under `fraction` of its error: the window that
    /// ends it once what it would keep is not worth carrying.
    pub fn exponential_window_secs(decay_ratio: f32, decay_period_secs: f32, fraction: f32) -> f32 {
        if decay_ratio <= 0.0 || decay_ratio >= 1.0 {
            return decay_period_secs.max(0.0);
        }
        decay_period_secs * fraction.ln() / decay_ratio.ln()
    }

    /// Fraction of the error still left, `elapsed` seconds in, for a curve
    /// running over a `duration`-long window.
    ///
    /// `duration` bounds every curve; an unbounded one ignores it for its shape
    /// and is cut off by it: nothing remains from the window's end.
    pub fn remaining(&self, elapsed: f32, duration: f32) -> f32 {
        match self {
            Self::Exponential {
                decay_ratio,
                decay_period_secs,
            } => {
                if *decay_period_secs <= 0.0 || elapsed >= duration {
                    return 0.0;
                }
                decay_ratio.powf(elapsed / decay_period_secs)
            }
            Self::Linear => bounded_remaining(EaseFunction::Linear, elapsed, duration),
            Self::Smoothstep => bounded_remaining(EaseFunction::SmoothStep, elapsed, duration),
            Self::EaseOutCubic => bounded_remaining(EaseFunction::CubicOut, elapsed, duration),
            Self::EaseInOutCubic => bounded_remaining(EaseFunction::CubicInOut, elapsed, duration),
        }
    }

    /// Per-frame keep ratio for the frame that starts `elapsed` seconds into a
    /// curve running over a `duration`-long window.
    ///
    /// This is `remaining(next) / remaining(prev)`, so the error telescopes along
    /// the curve exactly, whatever the frame rate. The frame covers
    /// `[elapsed, elapsed + dt]`, and counting the frame being decayed is what
    /// makes a blend's first frame already give up `1 - remaining(dt)` of the
    /// gap. Looking backwards instead clamps the previous edge to `0` on that
    /// frame, which makes the keep ratio exactly `1`: the rendered value is put
    /// back onto the one it already had, so the frame does not move at all.
    pub fn keep(&self, elapsed: f32, dt: f32, duration: f32) -> f32 {
        // The exponential's ratio is constant inside its window, so it does not need the edges;
        // the frame that reaches the window's end gives up the rest.
        if let Self::Exponential { .. } = self {
            if elapsed + dt >= duration {
                return 0.0;
            }
            return self.remaining(dt, f32::INFINITY);
        }
        let prev = self.remaining(elapsed, duration);
        let next = self.remaining(elapsed + dt, duration);
        if prev > 0.0 {
            (next / prev).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// What a bounded curve keeps of the error `elapsed` seconds into its `duration`-long window,
/// over which it is normalised.
fn bounded_remaining(curve: EaseFunction, elapsed: f32, duration: f32) -> f32 {
    if !duration.is_finite() || duration <= 0.0 {
        return 0.0;
    }
    1.0 - curve.sample_clamped(elapsed / duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each bounded curve starts with its whole error and ends with none, and halfway through
    /// keeps what its shape gives: linear and the symmetric curves a half, the cubic out
    /// (1 - 0.5)^3 = 0.125.
    #[test]
    fn bounded_curves_keep_their_shape() {
        for (ease, half) in [
            (CorrectionEase::Linear, 0.5),
            (CorrectionEase::Smoothstep, 0.5),
            (CorrectionEase::EaseOutCubic, 0.125),
            (CorrectionEase::EaseInOutCubic, 0.5),
        ] {
            assert_eq!(ease.remaining(0.0, 1.0), 1.0, "{ease:?}");
            assert_eq!(ease.remaining(0.5, 1.0), half, "{ease:?}");
            assert_eq!(ease.remaining(1.0, 1.0), 0.0, "{ease:?}");
        }
    }
}
