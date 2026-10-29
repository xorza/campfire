use crate::correction_ease::CorrectionEase;
use crate::on_timeline::OnTimeline;
use crate::pending_switch::PendingSwitch;
use crate::switch_direction::SwitchDirection;
use crate::timeline_switch_setup::TimelineSwitchSetup;
use bevy_ecs::prelude::*;
use core::time::Duration;
use serde::Deserialize;

/// Request to move an entity between the prediction and interpolation timelines.
///
/// Insert it on the entity — the switch pipeline is what adds or removes
/// [`Predicted`](campfire_net_replication::Predicted) /
/// [`Interpolated`](campfire_net_replication::Interpolated), so callers never touch those directly:
///
/// You can customize the blend length and ease curve per request, which overrides the
/// [`TimelineSwitchSetup`] default: a blend of no length snaps instantly,
/// otherwise the jump is smoothed over the window and further requests for the
/// entity are ignored until the switch finishes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct TimelineSwitch {
    /// The timeline to move to, or `None` to flip to whichever timeline the
    /// entity is not on. A bare [`TimelineSwitch::default()`] flips; an entity
    /// on neither timeline has nothing to flip from and the request is dropped.
    direction: Option<SwitchDirection>,
    /// Blend length, overriding [`TimelineSwitchSetup::default_transition`].
    transition: Option<Duration>,
    /// Ease curve, overriding [`TimelineSwitchSetup::default_ease`].
    ease: Option<CorrectionEase>,
}

impl TimelineSwitch {
    /// Switch this entity from interpolated to predicted
    pub fn to_predicted() -> Self {
        Self {
            direction: Some(SwitchDirection::ToPredicted),
            ..Default::default()
        }
    }

    /// Switch this entity from predicted to interpolated
    pub fn to_interpolated() -> Self {
        Self {
            direction: Some(SwitchDirection::ToInterpolated),
            ..Default::default()
        }
    }

    /// Override the [`TimelineSwitchSetup`] default blend length for this
    /// request. [`Duration::ZERO`] snaps instantly instead of blending.
    #[must_use]
    pub const fn with_transition(mut self, transition: Duration) -> Self {
        self.transition = Some(transition);
        self
    }

    /// Override the [`TimelineSwitchSetup`] default ease curve for this
    /// request.
    #[must_use]
    pub const fn with_ease(mut self, ease: CorrectionEase) -> Self {
        self.ease = Some(ease);
        self
    }

    /// Resolves the request against the timeline the entity is on and `setup` into the
    /// [`PendingSwitch`] the apply pass serves.
    pub(crate) fn resolve(
        &self,
        on: OnTimeline,
        setup: &TimelineSwitchSetup,
    ) -> Result<PendingSwitch, SwitchRejection> {
        let direction = match (self.direction, on) {
            (_, OnTimeline::Neither) => return Err(SwitchRejection::OnNeither),
            (Some(SwitchDirection::ToPredicted), OnTimeline::Predicted) => {
                return Err(SwitchRejection::NotInterpolated);
            }
            (Some(SwitchDirection::ToInterpolated), OnTimeline::Interpolated) => {
                return Err(SwitchRejection::NotPredicted);
            }
            (Some(direction), _) => direction,
            (None, OnTimeline::Predicted) => SwitchDirection::ToInterpolated,
            (None, OnTimeline::Interpolated) => SwitchDirection::ToPredicted,
        };
        Ok(PendingSwitch {
            direction,
            total: self.transition.unwrap_or(setup.default_transition),
            ease: self.ease.unwrap_or(setup.default_ease),
        })
    }
}

/// Why a [`TimelineSwitch`] request is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum SwitchRejection {
    /// The entity is on neither timeline, so it has nothing to switch from.
    OnNeither,
    /// A switch to prediction needs an interpolated entity.
    NotInterpolated,
    /// A switch to interpolation needs a predicted entity.
    NotPredicted,
}

impl core::fmt::Display for SwitchRejection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::OnNeither => "the entity is on neither timeline",
            Self::NotInterpolated => "a switch to predicted needs an interpolated entity",
            Self::NotPredicted => "a switch to interpolated needs a predicted entity",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A named switch needs the entity on the other timeline, a flip goes to the timeline the
    /// entity is not on, and an entity on neither is refused either way; the request's window
    /// overrides the setup's.
    #[test]
    fn a_request_resolves_against_the_timeline_it_leaves() {
        let setup = TimelineSwitchSetup::default();
        let requests = [
            TimelineSwitch::to_predicted(),
            TimelineSwitch::to_interpolated(),
            TimelineSwitch::default(),
        ];
        let resolved = |on| {
            requests.map(|request: TimelineSwitch| {
                request.resolve(on, &setup).map(|pending| pending.direction)
            })
        };
        assert_eq!(
            resolved(OnTimeline::Interpolated),
            [
                Ok(SwitchDirection::ToPredicted),
                Err(SwitchRejection::NotPredicted),
                Ok(SwitchDirection::ToPredicted)
            ]
        );
        assert_eq!(
            resolved(OnTimeline::Predicted),
            [
                Err(SwitchRejection::NotInterpolated),
                Ok(SwitchDirection::ToInterpolated),
                Ok(SwitchDirection::ToInterpolated)
            ]
        );
        assert_eq!(
            resolved(OnTimeline::Neither),
            [Err(SwitchRejection::OnNeither); 3]
        );
        let pending = TimelineSwitch::to_predicted()
            .with_transition(Duration::from_millis(1500))
            .resolve(OnTimeline::Interpolated, &setup)
            .unwrap();
        assert_eq!(pending.total, Duration::from_millis(1500));
        assert_ne!(setup.default_transition, Duration::from_millis(1500));
        assert_eq!(OnTimeline::of(true, true), OnTimeline::Predicted);
    }
}
