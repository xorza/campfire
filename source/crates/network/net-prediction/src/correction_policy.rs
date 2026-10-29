use core::fmt::Debug;
use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;

use crate::correction_ease::CorrectionEase;
use crate::visual_correction::CONVERGED;

/// Decay schedule for [`VisualCorrection`](crate::visual_correction::VisualCorrection) errors.
///
/// The default schedule lives on the `PredictionSetup` resource and applies
/// to every corrected entity. Inserting this component on an entity overrides
/// the global schedule for that entity only — that is how one-off corrections can
/// decay on their own tuning without retuning every other correction.
///
/// This is the unbounded, exponential easing ([`CorrectionEase::Exponential`]) written as a pair of
/// parameters, and it is what a rollback correction decays on: the aim there is to catch up with
/// the server, not to look deliberate. A live [`SwitchBlend`](crate::switch_blend::SwitchBlend)
/// window runs its own curve instead, and the two combine by taking whichever gives the error up
/// more slowly at that instant, so a blend is never shortened by this schedule and a rollback
/// arriving late in a window is never dumped by the curve's tail.
#[derive(Component, Debug, Clone, Copy, Reflect)]
pub struct CorrectionPolicy {
    /// How the error is given up.
    ///
    /// The default is the unbounded exponential, tuned in the two parameters
    /// below. A bounded curve ([`CorrectionEase::Smoothstep`] and friends) is
    /// normalised over `duration` instead, and reaches zero at its end.
    ease: CorrectionEase,
    /// How long the curve takes.
    ///
    /// A bounded curve is normalised over it and reaches zero at its end. An exponential
    /// carries its own period for its shape and is cut off here, which its constructors set to
    /// where it converges.
    duration: Duration,
}

impl Default for CorrectionPolicy {
    /// The shared default curve, so a rollback and a blend let the error go at the same rate
    /// unless a caller tunes one of them, run until it converges.
    fn default() -> Self {
        Self::exponential(
            CorrectionEase::DEFAULT_DECAY_RATIO,
            CorrectionEase::DEFAULT_DECAY_PERIOD,
        )
    }
}

impl CorrectionPolicy {
    /// Custom exponential schedule: after each `decay_period`, `decay_ratio` of
    /// the error remains. Insert on an entity to override the global schedule for
    /// that entity only.
    ///
    /// It runs until it keeps under a thousandth of its error, the visual correction's
    /// convergence: the window that ends it.
    pub fn new(decay_ratio: f32, decay_period: Duration) -> Self {
        Self::exponential(decay_ratio, decay_period)
    }

    fn exponential(decay_ratio: f32, decay_period: Duration) -> Self {
        Self {
            ease: CorrectionEase::Exponential {
                decay_ratio,
                decay_period,
            },
            duration: CorrectionEase::exponential_window(decay_ratio, decay_period, CONVERGED),
        }
    }

    /// Any curve, run over `duration`.
    ///
    /// This is how a bounded curve is selected: an exponential ignores the
    /// duration and is only bounded by it, while a bounded one is normalised over
    /// it and reaches zero there.
    pub const fn with_ease(ease: CorrectionEase, duration: Duration) -> Self {
        Self { ease, duration }
    }

    /// The curve this policy decays on.
    pub const fn ease(&self) -> CorrectionEase {
        self.ease
    }

    /// The lerp constant to use for decaying the error in a framestep-insensitive way.
    ///
    /// For an exponential this is constant, which is the classic framestep-insensitive
    /// exponential decay; see <https://www.youtube.com/watch?v=LSNQuFEDOyQ>.
    /// A bounded curve has no such constant — its ratio depends on where in the
    /// window the frame falls — so it is read from `elapsed`, which the caller
    /// takes from the correction's own recorded start.
    #[inline]
    pub fn lerp_ratio(&self, elapsed: Duration, delta: Duration) -> f32 {
        self.ease.keep(elapsed, delta, self.duration)
    }
}
