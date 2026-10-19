use crate::correction_ease::CorrectionEase;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use core::time::Duration;

/// Marks an entity while a switch is ongoing.
///
/// While present, a new [`TimelineSwitch`](crate::timeline_switch::TimelineSwitch) inserted on the
/// entity is dropped.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
pub struct SwitchBlend {
    /// Visual-clock reading the blend started at.
    start: Duration,
    /// Requested blend length in seconds.
    total_secs: f32,
    /// Ease curve shaping the convergence over the window.
    ease: CorrectionEase,
}

impl SwitchBlend {
    /// Stamps the shared window for one switch: `now` is the current
    /// visual-clock reading, `total_secs` the requested blend length.
    pub(crate) const fn new(now: Duration, total_secs: f32, ease: CorrectionEase) -> Self {
        Self {
            start: now,
            total_secs,
            ease,
        }
    }

    /// Requested blend length in seconds.
    pub const fn total_secs(&self) -> f32 {
        self.total_secs
    }

    /// Blend time elapsed in seconds at the `now` clock reading.
    pub const fn elapsed_secs(&self, now: Duration) -> f32 {
        now.saturating_sub(self.start).as_secs_f32()
    }

    /// True once the window has run out at the `now` clock reading.
    pub fn is_expired(&self, now: Duration) -> bool {
        self.elapsed_secs(now) >= self.total_secs
    }

    /// Fraction of the window's recorded jump to keep over the frame of
    /// `dt_secs` that starts at `now`, or `None` when the window cannot
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
    /// run out, or its length is unusable.
    pub(crate) fn curve_keep(&self, now: Duration, dt_secs: f32) -> Option<f32> {
        let total = self.total_secs;
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let elapsed = self.elapsed_secs(now);
        if elapsed >= total {
            return None;
        }
        Some(self.ease.keep(elapsed, dt_secs, total))
    }

    /// Ease curve shaping the convergence over the window.
    pub const fn ease(&self) -> CorrectionEase {
        self.ease
    }
}
