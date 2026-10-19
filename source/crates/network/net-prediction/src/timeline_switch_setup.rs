use crate::correction_ease::CorrectionEase;
use bevy_ecs::prelude::*;

/// Default blend length for timeline switches, in seconds.
///
/// The drain resolves a request without an explicit override against this
/// resource, so games tune the feel in one place instead of threading a
/// duration through every gameplay insert site. Insert (or mutate) it to change
/// the default; per-request `TimelineSwitch::with_transition_secs` wins.
/// Same for the ease curve (`TimelineSwitch::with_ease`).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct TimelineSwitchSetup {
    /// Blend length used when a request carries no explicit override.
    pub default_transition_secs: f32,
    /// Ease curve used when a request carries no explicit override.
    pub default_ease: CorrectionEase,
}

impl Default for TimelineSwitchSetup {
    fn default() -> Self {
        Self {
            // Long enough for a blend to converge: with the default curve, half
            // the gap goes every 200 ms, so a second leaves about 3% of it, which
            // the correction policy finishes.
            default_transition_secs: 1.0,
            default_ease: CorrectionEase::default(),
        }
    }
}
