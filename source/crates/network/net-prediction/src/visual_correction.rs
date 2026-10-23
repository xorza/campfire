use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use core::fmt::Debug;
use core::time::Duration;

/// The visual correction ends once it keeps less than this fraction of its recorded jump: a
/// thousandth of a jump is below what a render shows, and an f32 decay that went on would only
/// stall at denormals instead of reaching zero.
pub(crate) const CONVERGED: f32 = 1.0 / 1024.0;

/// The error a predicted entity's render carries while a correction decays; `None` while no
/// correction does.
///
/// The slot goes on with a corrected component's prediction history and stays, so a correction
/// that starts or converges moves no entity between tables.
#[derive(Component, Debug, Reflect)]
pub struct VisualCorrection<D>(pub Option<VisualOffset<D>>);

impl<D> Default for VisualCorrection<D> {
    fn default() -> Self {
        Self(None)
    }
}

/// One decaying correction.
#[derive(Debug, Reflect)]
pub struct VisualOffset<D> {
    /// The error between the original visual value and the new visual value.
    /// Will decay over time.
    ///
    /// It is an offset: the apply adds it to the visual the frame wrote, every frame, so what is
    /// rendered is `visual + error`, and the decay shrinks it towards zero.
    ///
    /// A live [`SwitchBlend`](crate::switch_blend::SwitchBlend) window shapes that decay instead of
    /// the entity's `CorrectionPolicy`.
    pub error: D,
    /// The virtual-clock reading this error was recorded at.
    ///
    /// A bounded curve needs to know how far into its window it is, so it is
    /// measured from here. The exponential does not (its ratio is constant), but
    /// it is recorded for every correction so the apply has one shape to read.
    pub(crate) start: Duration,
    /// The fraction of the recorded jump the error still holds: the product of every frame's
    /// keep ratio, which [`CONVERGED`] ends.
    pub(crate) remaining: f32,
}

impl<D> VisualOffset<D> {
    /// A correction decaying on the entity's `CorrectionPolicy`, starting now.
    pub const fn new(error: D, start: Duration) -> Self {
        Self {
            error,
            start,
            remaining: 1.0,
        }
    }

    /// How long this error has been decaying at the `now` clock reading.
    pub const fn elapsed(&self, now: Duration) -> Duration {
        now.saturating_sub(self.start)
    }
}

#[cfg(test)]
impl<D> VisualCorrection<D> {
    /// A slot that holds a correction of `error` from `start`.
    pub(crate) const fn active(error: D, start: Duration) -> Self {
        Self(Some(VisualOffset::new(error, start)))
    }
}
