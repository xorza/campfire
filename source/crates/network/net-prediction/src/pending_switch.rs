use crate::correction_ease::CorrectionEase;
use crate::switch_direction::SwitchDirection;
use bevy_ecs::prelude::*;
use core::time::Duration;

/// A resolved [`TimelineSwitch`], committed for the next frame's apply pass.
///
/// While this is present, a new [`TimelineSwitch`] is ignored, exactly as it is
/// while a [`SwitchBlend`] window is running.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingSwitch {
    /// The timeline to move to, resolved against the entity's markers.
    pub(crate) direction: SwitchDirection,
    /// Blend length, resolved against [`TimelineSwitchSetup`]; none snaps.
    pub(crate) total: Duration,
    /// Ease curve shaping the blend, resolved.
    pub(crate) ease: CorrectionEase,
}

impl PendingSwitch {
    /// Whether this switch blends instead of snapping.
    pub(crate) const fn blends(&self) -> bool {
        !self.total.is_zero()
    }
}
