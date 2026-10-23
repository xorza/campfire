use crate::interpolation_fn::InterpolationFn;
use crate::into_interpolation_fn::{IntoCxLerpFn, IntoLerpFn};
use bevy_curve::Ease;
use bevy_ecs::component::Component;
use campfire_net_frame_interpolation::linear_lerp;
use core::marker::PhantomData;

/// Functions used by an interpolation rule.
///
/// The constructors describe which work the stack holds:
///
/// - [`Self::with`] stores received values in
///   [`ConfirmedHistory`](campfire_net_replication::ConfirmedHistory),
///   prepares that history, samples it, and applies the result to the live
///   component. For bundle interpolation, each component is stored in its own
///   history before the tuple interpolation function is called.
/// - [`Self::history_only`] stores and prepares history but does not apply the
///   live component. This is the usual choice when a user system performs
///   the interpolation: the stack populates the history, and the user's own
///   system interpolates.
/// - [`Self::disabled`] registers no history and no interpolation function. Use
///   it as a high-priority filtered rule to opt matching entities out of a
///   broader interpolation rule.
#[derive(Debug, Clone)]
pub struct InterpolationFns<C> {
    pub(crate) interpolation: Option<InterpolationFn<C>>,
    pipeline: InterpolationPipeline,
    _marker: PhantomData<fn(C)>,
}

/// Which pipeline stages a rule holds.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InterpolationPipeline {
    Full,
    HistoryOnly,
    Disabled,
}

impl InterpolationPipeline {
    pub(crate) const fn holds_interpolation_history(self) -> bool {
        matches!(self, Self::Full | Self::HistoryOnly)
    }
}

impl<C> InterpolationFns<C> {
    /// Enables the full delayed-interpolation pipeline for `C`.
    ///
    /// Incoming updates are stored in
    /// [`ConfirmedHistory<C>`](campfire_net_replication::ConfirmedHistory),
    /// prepared every frame, and the provided interpolation function is used to
    /// apply the live component at the interpolation timeline.
    pub fn with<M>(interpolation: impl IntoLerpFn<C, M>) -> Self {
        Self {
            interpolation: Some(interpolation.into_interpolation_fn()),
            pipeline: InterpolationPipeline::Full,
            _marker: PhantomData,
        }
    }

    /// Enables the full delayed-interpolation pipeline with a callback that
    /// receives the sample interval.
    ///
    /// Use this for interpolation such as Hermite curves whose endpoint
    /// derivatives must be scaled by the duration between the sampled states.
    pub fn with_cx<M>(interpolation: impl IntoCxLerpFn<C, M>) -> Self {
        Self {
            interpolation: Some(interpolation.into_interpolation_fn()),
            pipeline: InterpolationPipeline::Full,
            _marker: PhantomData,
        }
    }

    /// Stores and prepares interpolation history, but does not apply `C`.
    ///
    /// Use this when the stack should receive component updates into
    /// [`ConfirmedHistory<C>`](campfire_net_replication::ConfirmedHistory), but visible
    /// interpolation is handled by a user system. For example, a system may sample several
    /// histories and write a render-only component.
    pub fn history_only() -> Self {
        Self {
            interpolation: None,
            pipeline: InterpolationPipeline::HistoryOnly,
            _marker: PhantomData,
        }
    }

    /// Disables stack interpolation work for matching entities.
    ///
    /// A disabled high-priority rule can be used to exclude a filtered set of
    /// entities from a broader default interpolation rule. If the disabled rule
    /// is selected for an archetype, the stack does not fall back to lower
    /// priority matching rules for that component.
    ///
    /// This is useful when most entities should interpolate but a marked subset
    /// should snap, be driven by a custom visual system, or temporarily opt out
    /// during a mode change.
    pub fn disabled() -> Self {
        Self {
            interpolation: None,
            pipeline: InterpolationPipeline::Disabled,
            _marker: PhantomData,
        }
    }

    pub(crate) const fn pipeline(&self) -> InterpolationPipeline {
        self.pipeline
    }

    pub(crate) const fn holds_interpolation_history(&self) -> bool {
        self.pipeline.holds_interpolation_history()
    }

    pub(crate) const fn applies_interpolation_component(&self) -> bool {
        matches!(self.pipeline, InterpolationPipeline::Full)
    }

    /// The full pipeline, with a linear [`Ease`] interpolation.
    pub fn linear() -> Self
    where
        C: Component + Ease + Clone,
    {
        Self::with(linear_lerp::<C>)
    }
}
