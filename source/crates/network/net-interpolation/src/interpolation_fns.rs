use crate::interpolation_fn::InterpolationFn;
use crate::into_interpolation_fn::{IntoContextLerpFn, IntoLerpFn};
use crate::lerps::linear_lerp;
use bevy_curve::Ease;
use bevy_ecs::component::Component;
use core::marker::PhantomData;

/// Functions used by an interpolation rule.
///
/// The constructors describe which work the stack owns:
///
/// - [`Self::with`] stores received values in
///   [`ConfirmedHistory`](campfire_net_replication::ConfirmedHistory),
///   prepares that history, samples it, and applies the result to the live
///   component. For bundle interpolation, each component is stored in its own
///   history before the tuple interpolation function is called.
/// - [`Self::history_only`] stores and prepares history but does not apply the
///   live component. This is the usual choice when a user system performs
///   the interpolation: the stack populates the history, and the user's own
///   system interpolates. You can still
///   specify a default interpolation fn that can be used for other purposes, such
///   as frame interpolation.
/// - [`Self::no_history`] registers an interpolation function without owning
///   delayed interpolation history or delayed live-component presence. Frame
///   interpolation and correction can still reuse the same rule.
/// - [`Self::disabled`] registers no history and no interpolation function. Use
///   it as a high-priority filtered rule to opt matching entities out of a
///   broader interpolation rule.
///
/// # Examples
///
/// Use the full delayed-interpolation pipeline:
///
/// ```rust,ignore
/// use bevy_ecs::prelude::*;
/// use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
///
/// #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
/// struct Position(f32);
///
/// fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
///     Position(start.0 + (end.0 - start.0) * t)
/// }
///
/// app.register_component::<Position>()
///     .replicate()
///     .interpolate(InterpolationFns::with(lerp_position));
/// ```
///
/// Keep histories but interpolate in your own system:
///
/// ```rust,ignore
/// use bevy_ecs::prelude::*;
/// use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt, InterpolationSystems};
///
/// #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
/// struct Position(f32);
///
/// app.register_component::<Position>()
///     .replicate()
///     .interpolate(InterpolationFns::history_only());
///
/// app.add_systems(Update, custom_interpolation.in_set(InterpolationSystems::Interpolate));
/// ```
#[derive(Debug, Clone)]
pub struct InterpolationFns<C> {
    pub(crate) interpolation: Option<InterpolationFn<C>>,
    pipeline: InterpolationPipeline,
    _marker: PhantomData<fn(C)>,
}

/// Which pipeline stages a rule owns.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InterpolationPipeline {
    Full,
    HistoryOnly,
    NoHistory,
    Disabled,
}

impl InterpolationPipeline {
    pub(crate) const fn owns_interpolation_history(self) -> bool {
        matches!(self, Self::Full | Self::HistoryOnly)
    }

    pub(crate) const fn owns_frame_history(self) -> bool {
        matches!(self, Self::Full | Self::HistoryOnly | Self::NoHistory)
    }
}

impl<C> InterpolationFns<C> {
    /// Enables the full delayed-interpolation pipeline for `C`.
    ///
    /// Incoming updates are stored in
    /// [`ConfirmedHistory<C>`](campfire_net_replication::ConfirmedHistory),
    /// prepared every frame, and the provided interpolation function is used to
    /// apply the live component at the interpolation timeline.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use bevy_ecs::prelude::*;
    /// # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
    /// # #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
    /// # struct Position(f32);
    /// fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
    ///     Position(start.0 + (end.0 - start.0) * t)
    /// }
    ///
    /// app.register_component::<Position>()
    ///     .replicate()
    ///     .interpolate(InterpolationFns::with(lerp_position));
    /// ```
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
    pub fn with_context<M>(interpolation: impl IntoContextLerpFn<C, M>) -> Self {
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
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use bevy_ecs::prelude::*;
    /// # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
    /// # #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
    /// # struct Position(f32);
    /// app.register_component::<Position>()
    ///     .replicate()
    ///     .interpolate(InterpolationFns::history_only());
    /// ```
    pub fn history_only() -> Self {
        Self {
            interpolation: None,
            pipeline: InterpolationPipeline::HistoryOnly,
            _marker: PhantomData,
        }
    }

    /// Registers an interpolation function without delayed interpolation history.
    ///
    /// This is useful when a component is not delayed-interpolated through
    /// [`Interpolated`](campfire_net_replication::Interpolated), but entities
    /// with [`FrameInterpolate`](crate::frame_interpolate::FrameInterpolate)
    /// should still reuse the same interpolation function for visual smoothing
    /// and correction. Unlike [`Self::disabled`], this still participates in
    /// frame interpolation and correction because it stores an interpolation
    /// function.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use bevy_ecs::prelude::*;
    /// # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
    /// # #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
    /// # struct Position(f32);
    /// # fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
    /// #     Position(start.0 + (end.0 - start.0) * t)
    /// # }
    /// app.register_component::<Position>()
    ///     .replicate()
    ///     .interpolate(InterpolationFns::no_history(lerp_position));
    /// ```
    pub fn no_history<M>(interpolation: impl IntoLerpFn<C, M>) -> Self {
        Self {
            interpolation: Some(interpolation.into_interpolation_fn()),
            pipeline: InterpolationPipeline::NoHistory,
            _marker: PhantomData,
        }
    }

    /// Registers a context-aware interpolation function without delayed
    /// interpolation history.
    pub fn no_history_with_context<M>(interpolation: impl IntoContextLerpFn<C, M>) -> Self {
        Self {
            interpolation: Some(interpolation.into_interpolation_fn()),
            pipeline: InterpolationPipeline::NoHistory,
            _marker: PhantomData,
        }
    }

    /// Disables stack interpolation work for matching entities.
    ///
    /// A disabled high-priority rule can be used to exclude a filtered set of
    /// entities from a broader default interpolation rule. If the disabled rule
    /// is selected for an archetype, the stack does not fall back to lower
    /// priority matching rules for that component. Unlike [`Self::no_history`],
    /// this does not register an interpolation function for frame interpolation
    /// or correction.
    ///
    /// This is useful when most entities should interpolate but a marked subset
    /// should snap, be driven by a custom visual system, or temporarily opt out
    /// during a mode change.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use bevy_ecs::prelude::*;
    /// # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt, InterpolationRule};
    /// # #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
    /// # struct Position(f32);
    /// #[derive(Component)]
    /// struct SnapOnly;
    ///
    /// app.register_component::<Position>()
    ///     .replicate()
    ///     .interpolate(InterpolationFns::linear());
    ///
    /// // Entities with `SnapOnly` match both rules. This rule has higher
    /// // priority, so it blocks the broader default `Position` interpolation.
    /// app.register_interpolation_rule::<Position, With<SnapOnly>>(
    ///     InterpolationRule::new(InterpolationFns::disabled()).priority(100),
    /// );
    /// ```
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

    pub(crate) const fn owns_interpolation_history(&self) -> bool {
        self.pipeline.owns_interpolation_history()
    }

    pub(crate) const fn applies_interpolation_component(&self) -> bool {
        matches!(self.pipeline, InterpolationPipeline::Full)
    }

    pub(crate) const fn applies_frame_component(&self) -> bool {
        self.interpolation.is_some() && self.pipeline.owns_frame_history()
    }
}

#[expect(
    clippy::return_self_not_must_use,
    reason = "a builder of rule fns, which the registration consumes"
)]
impl<C> InterpolationFns<C> {
    /// The full pipeline, with a linear [`Ease`] interpolation.
    pub fn linear() -> Self
    where
        C: Component + Ease + Clone,
    {
        Self::with(linear_lerp::<C>)
    }

    /// Stores `interpolation` on this rule without changing which pipeline stages the rule
    /// owns: most useful with [`Self::history_only`], where the stack owns history and presence
    /// but a user system owns delayed interpolation, while frame interpolation and correction
    /// can still reuse the interpolation function.
    pub fn interpolate<M>(mut self, interpolation: impl IntoLerpFn<C, M>) -> Self {
        self.interpolation = Some(interpolation.into_interpolation_fn());
        self
    }

    /// Stores a context-aware interpolation callback on this rule without changing which
    /// pipeline stages the rule owns.
    pub fn interpolate_with_context<M>(
        mut self,
        interpolation: impl IntoContextLerpFn<C, M>,
    ) -> Self {
        self.interpolation = Some(interpolation.into_interpolation_fn());
        self
    }

    /// Stores a linear [`Ease`] interpolation function on this rule.
    pub fn linear_interpolate(self) -> Self
    where
        C: Component + Ease + Clone,
    {
        self.interpolate(linear_lerp::<C>)
    }
}
