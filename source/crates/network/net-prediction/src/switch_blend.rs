use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;

use crate::correction_ease::CorrectionEase;

/// Marks an entity while a switch is ongoing.
///
/// While present, a new [`TimelineSwitch`](crate::timeline_switch::TimelineSwitch) inserted on the
/// entity is dropped.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
pub struct SwitchBlend {
    /// Visual-clock reading the blend started at.
    start: Duration,
    /// Requested blend length.
    total: Duration,
    /// Ease curve shaping the convergence over the window.
    ease: CorrectionEase,
}

impl SwitchBlend {
    /// Stamps the shared window for one switch: `now` is the current
    /// visual-clock reading, `total` the requested blend length.
    pub(crate) const fn new(now: Duration, total: Duration, ease: CorrectionEase) -> Self {
        Self {
            start: now,
            total,
            ease,
        }
    }

    /// Requested blend length.
    pub const fn total(&self) -> Duration {
        self.total
    }

    /// Blend time elapsed at the `now` clock reading.
    pub const fn elapsed(&self, now: Duration) -> Duration {
        now.saturating_sub(self.start)
    }

    /// True once the window has run out at the `now` clock reading.
    pub fn is_expired(&self, now: Duration) -> bool {
        self.elapsed(now) >= self.total
    }

    /// Fraction of the window's recorded jump to keep over the frame of
    /// `dt` that starts at `now`, or `None` when the window cannot
    /// shape a decay.
    ///
    /// The error decays rather than being recomputed, so this is the per-frame
    /// scale factor: `remaining(next) / remaining(prev)` follows the ease curve
    /// exactly, whatever the frame rate. The frame covers
    /// `[now, now + dt]`, counting the frame being decayed: a window's first
    /// frame already gives up `1 - remaining(dt)` of the gap instead of keeping
    /// all of it, so the render keeps moving through the switch.
    ///
    /// `None` means the caller should fall back to the entity's own
    /// `CorrectionPolicy`: the window has
    /// run out, or it has no length.
    pub(crate) fn curve_keep(&self, now: Duration, dt: Duration) -> Option<f32> {
        let elapsed = self.elapsed(now);
        if self.total.is_zero() || elapsed >= self.total {
            return None;
        }
        Some(self.ease.keep(elapsed, dt, self.total))
    }

    /// Ease curve shaping the convergence over the window.
    pub const fn ease(&self) -> CorrectionEase {
        self.ease
    }
}
