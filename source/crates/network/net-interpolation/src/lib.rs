//! Interpolation for replicated entities.
//!
//! Network replication delivers state at discrete server ticks. Rendering those updates directly
//! can make remote entities snap from one received value to the next. This crate keeps a delayed
//! history of received component values and applies interpolated values on entities with
//! [`Interpolated`](campfire_net_replication::Interpolated).
//!
//! # Basic setup
//!
//! Add [`interpolation_plugin::InterpolationPlugin`] on the client and register interpolation rules
//! in the same shared protocol code that registers replicated components. The rules are stored in
//! [`interpolation_registry::InterpolationRegistry`].
//!
//! A full interpolation rule owns the whole delayed-interpolation pipeline for a
//! component:
//!
//! - received values are stored in
//!   [`ConfirmedHistory<C>`](campfire_net_replication::ConfirmedHistory),
//! - the history is sampled at the client's [`interpolation_timeline::InterpolationTimeline`],
//! - and the sampled value is written back to the live component.
//!
//! Applying a sampled value uses Bevy's normal component change detection, so
//! systems ordered after interpolation can observe it with `Changed<C>`.
//!
//! ```rust,ignore
//! use bevy_app::App;
//! use bevy_ecs::prelude::*;
//! use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
//! use campfire_net_replication::{AppComponentExt, VisibilityExt};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Position(f32);
//!
//! fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
//!     Position(start.0 + (end.0 - start.0) * t)
//! }
//!
//! fn protocol(app: &mut App) {
//!     app.register_component::<Position>()
//!         .replicate()
//!         .interpolate(InterpolationFns::with(lerp_position));
//! }
//! ```
//!
//! If `C` implements [`Ease`](bevy_curve::Ease), the common linear case is
//! [`interpolation_fns::InterpolationFns::linear`]:
//!
//! ```rust,ignore
//! # use bevy_app::App;
//! # use bevy_ecs::prelude::*;
//! # use bevy_curve::{Curve, Ease};
//! # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt};
//! # use campfire_net_replication::{AppComponentExt, VisibilityExt};
//! # use serde::{Deserialize, Serialize};
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Position(f32);
//!
//! impl Ease for Position {
//!     fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
//!         bevy_curve::FunctionCurve::new(bevy_curve::Interval::UNIT, move |t| {
//!             Position(start.0 + (end.0 - start.0) * t)
//!         })
//!     }
//! }
//!
//! fn protocol(app: &mut App) {
//!     app.register_component::<Position>()
//!         .replicate()
//!         .interpolate(InterpolationFns::linear());
//! }
//! ```
//!
//! # Interpolation functions
//!
//! [`interpolation_fns::InterpolationFns`] describes which work the stack owns for a rule:
//!
//! - [`interpolation_fns::InterpolationFns::with`] owns delayed history and applies the live
//!   component.
//! - [`interpolation_fns::InterpolationFns::history_only`] owns delayed history but does not apply
//!   the live component. This is the usual choice when a user system samples
//!   one or more histories and writes visuals itself.
//! - [`interpolation_fns::InterpolationFns::history_only`] plus
//!   [`interpolation_fns::InterpolationFns::interpolate`] keeps an interpolation function for
//!   frame interpolation and correction, while still leaving delayed
//!   interpolation application to user code.
//! - [`interpolation_fns::InterpolationFns::no_history`] stores only an interpolation function. It
//!   is useful for predicted entities that need frame interpolation or visual
//!   correction but do not receive delayed interpolation history.
//! - [`interpolation_fns::InterpolationFns::disabled`] is a high-priority opt-out rule. If it is
//!   selected, lower-priority matching rules for the same component or bundle do
//!   not run.
//!
//! Components used with these rules must implement [`HistoryComponent`]:
//! `Component<Mutability = Mutable> + Clone + PartialEq`.
//!
//! Most interpolation functions use the simple `fn(start, end, fraction)` shape. Functions that
//! need sample timing can be registered with [`interpolation_fns::InterpolationFns::with_context`];
//! they receive [`interpolation_sample_context::InterpolationSampleContext`], which includes the
//! normalized fraction plus sample duration when it is available. This is useful for interpolation
//! methods such as Hermite curves that need to scale velocities by the interval between samples.
//!
//! # User interpolation systems
//!
//! Use a history-only rule when the stack should still receive and maintain the
//! delayed history, but your own system decides how to apply it. This is useful
//! when interpolation depends on several components, render-only state, or game
//! specific constraints.
//!
//! ```rust,ignore
//! use bevy_app::{App, Update};
//! use bevy_ecs::prelude::*;
//! use campfire_net_replication::ConfirmedHistory;
//! use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt, InterpolationSystems};
//! use campfire_net_replication::{AppComponentExt, ConfirmedHistory, Interpolated, VisibilityExt};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Position(f32);
//!
//! #[derive(Component)]
//! struct VisualPosition(f32);
//!
//! fn protocol(app: &mut App) {
//!     app.register_component::<Position>()
//!         .replicate()
//!         .interpolate(InterpolationFns::history_only());
//!     app.add_systems(Update, custom_interpolation.in_set(InterpolationSystems::Interpolate));
//! }
//!
//! fn custom_interpolation(
//!     mut query: Query<(&ConfirmedHistory<Position>, &mut VisualPosition), With<Interpolated>>,
//! ) {
//!     for (history, mut visual) in &mut query {
//!         if let Some(oldest) = history.get_nth_present(0) {
//!             visual.0 = oldest.value.0;
//!         }
//!     }
//! }
//! ```
//!
//! The example intentionally keeps the sampling simple. Real systems usually use
//! [`interpolation_timeline::InterpolationTimeline`] and [`interpolation_fraction`] to
//! choose the bracketing samples and interpolation fraction.
//!
//! # Rules, filters, and priority
//!
//! A rule is registered for a rule kind: either a single component `C` or a
//! tuple bundle such as `(Position, Rotation)`. Rules can also include an
//! [`ArchetypeFilter`](bevy_ecs::query::ArchetypeFilter), such as
//! `With<MyMarker>` or `Without<MyMarker>`.
//!
//! For each interpolated archetype, the stack considers every matching rule in
//! this order:
//!
//! 1. higher priority first,
//! 2. then earlier registration order for equal priority.
//!
//! Rule resolution assigns history maintenance and interpolation application
//! separately. Within each assignment, a selected rule atomically claims all
//! of its component members. A lower-priority component or bundle rule is
//! skipped if any of those members were already claimed for the same kind of
//! work.
//!
//! The default priority is the number of components in the interpolation target.
//! A default `(Position, Rotation)` bundle therefore takes priority over default
//! single-component `Position` and `Rotation` rules when all match the same
//! archetype.
//!
//! ## History ownership and apply ownership
//!
//! A registered rule describes both how its members' histories are maintained
//! and how those members are interpolated. Resolution can assign those two
//! responsibilities to different matching rules, however. This is necessary
//! because delayed component insertion and removal intentionally allow a
//! history component to exist while its corresponding live component does not.
//!
//! Resolution uses two independent kinds of ownership:
//!
//! | Ownership | A rule is eligible when... |
//! | --- | --- |
//! | History | Every member has either its live component or the relevant history component. |
//! | Apply | Every member has its live component. |
//!
//! Rules are considered in the priority and registration order described
//! above. History and apply each have their own set of claimed members, so one
//! rule can own history, apply, or both. Claiming a member for history does not
//! prevent another rule from claiming it for apply, and vice versa.
//! "History ownership" includes the decision not to keep history: when a
//! [`interpolation_fns::InterpolationFns::no_history`] rule wins that lane, it has no history
//! callbacks to run and prevents a lower-priority rule from creating history
//! for the same members.
//!
//! For example, suppose a higher-priority bundle rule targets `(A, B)` and a
//! lower-priority component rule targets `A`:
//!
//! - During a delayed insertion, an entity can have live `A` plus histories for
//!   both `A` and `B`, while live `B` is waiting for its insertion tick. The
//!   bundle owns history maintenance, while the component rule applies `A`.
//!   When history maintenance inserts `B`, the bundle becomes eligible to apply
//!   both components.
//! - During a delayed removal, the bundle owns both responsibilities while `A`
//!   and `B` are live. Once history maintenance reaches the removal tick and
//!   removes `B`, the bundle continues to own both histories, while the
//!   component rule takes over applying the remaining live `A`.
//!
//! Missing history is different from a missing live component. If both `A` and
//! `B` are live but one history for a history-based bundle has not been created
//! yet, the bundle is still the apply winner. It creates and fills the missing
//! history, and apply waits until all required samples are queryable; a
//! lower-priority `A` rule does not temporarily take over merely because the
//! bundle cannot sample yet.
//!
//! ```rust,ignore
//! # use bevy_app::App;
//! # use bevy_ecs::prelude::*;
//! # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt, InterpolationRule};
//! # use campfire_net_replication::{AppComponentExt, VisibilityExt};
//! # use serde::{Deserialize, Serialize};
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Position(f32);
//!
//! #[derive(Component)]
//! struct SnapPosition;
//!
//! fn lerp_position(start: &Position, end: &Position, t: f32) -> Position {
//!     Position(start.0 + (end.0 - start.0) * t)
//! }
//!
//! fn protocol(app: &mut App) {
//!     app.register_component::<Position>()
//!         .replicate()
//!         .interpolate(InterpolationFns::with(lerp_position));
//!
//!     // Entities with `SnapPosition` match both rules. This higher-priority
//!     // disabled rule blocks interpolation for those archetypes.
//!     app.register_interpolation_rule::<Position, With<SnapPosition>>(
//!         InterpolationRule::new(InterpolationFns::disabled()).priority(100),
//!     );
//! }
//! ```
//!
//! # Bundle interpolation
//!
//! Bundle rules let one interpolation function sample several component
//! histories together. Each component still has its own
//! [`ConfirmedHistory`](campfire_net_replication::ConfirmedHistory), but the
//! apply step fetches all member histories and calls the tuple interpolation
//! function.
//!
//! ```rust,ignore
//! # use bevy_app::App;
//! # use bevy_ecs::prelude::*;
//! # use campfire_net_interpolation::{AppInterpolationExt, InterpolationFns, InterpolationRegistrationExt, InterpolationRule};
//! # use campfire_net_replication::{AppComponentExt, VisibilityExt};
//! # use serde::{Deserialize, Serialize};
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Position(f32);
//!
//! #[derive(Component, Clone, PartialEq, Serialize, Deserialize)]
//! struct Rotation(f32);
//!
//! fn interpolate_transform(
//!     start: (&Position, &Rotation),
//!     end: (&Position, &Rotation),
//!     t: f32,
//! ) -> (Position, Rotation) {
//!     (
//!         Position(start.0.0 + (end.0.0 - start.0.0) * t),
//!         Rotation(start.1.0 + (end.1.0 - start.1.0) * t),
//!     )
//! }
//!
//! fn protocol(app: &mut App) {
//!     app.register_component::<Position>().replicate();
//!     app.register_component::<Rotation>().replicate();
//!     app.register_bundle_interpolation_rule::<(Position, Rotation), ()>(
//!         InterpolationRule::new(InterpolationFns::with(interpolate_transform)),
//!     );
//! }
//! ```
//!
//! Bundle interpolation is supported for tuple sizes up to 8.
//!
//! # Frame interpolation and correction
//!
//! The same interpolation rules are also reused by frame interpolation and
//! post-rollback visual correction when those plugins are enabled. In the common
//! case, register one interpolation rule in the protocol and add
//! [`FrameInterpolate`] to predicted
//! or visual entities that should be smoothed between fixed ticks.
//!
//! If an entity should use a rule only for delayed interpolation, or only for
//! frame interpolation, use filtered rules with marker components such as
//! [`Interpolated`](campfire_net_replication::Interpolated) or
//! [`FrameInterpolate`].
//!
//! # Scheduling
//!
//! [`interpolation_plugin::InterpolationPlugin`] runs the receive-history and apply systems in
//! [`interpolation_systems::InterpolationSystems`]. User interpolation systems should usually run
//! in [`interpolation_systems::InterpolationSystems::Interpolate`] so they see histories after the
//! stack has updated component presence for the interpolation timeline.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_interpolation_ext;
mod archetype_set;
mod confirmed_history_fns;
mod delayed_interpolation_plugin;
/// Handles delayed despawns for interpolated entities.
mod despawn;
mod frame_history_fns;
mod frame_interpolation_plugin;
mod frame_interpolation_rules;
mod frame_interpolation_systems;
mod history_component;
mod history_receive;
mod history_seed;
#[cfg(feature = "internals")]
pub mod internals;
/// Contains interpolation logic.
mod interpolate;
mod interpolation_bundle;
mod interpolation_config;
mod interpolation_fn;
mod interpolation_fns;
mod interpolation_marker_plugin;
mod interpolation_mode;
mod interpolation_owners;
/// Provides the `InterpolationPlugin` and related systems for Bevy integration.
mod interpolation_plugin;
mod interpolation_registration_ext;
mod interpolation_registry;
mod interpolation_registry_plugin;
mod interpolation_rule;
mod interpolation_rule_component;
mod interpolation_rule_id;
mod interpolation_sample_context;
mod interpolation_stage;
mod interpolation_state;
mod interpolation_systems;
mod interpolation_timeline;
mod interpolation_timeline_plugin;
mod into_interpolation_fn;
mod lerps;
mod member_mode;
mod registered_rule;
mod rule_building;
mod rule_resolution;
mod skip_frame_interpolation;
mod synced_interpolation_timeline;
mod update_history_context;

/// Commonly used items for client-side interpolation.
mod frame_interpolate;
mod frame_interpolation_history;
mod frame_source;
mod frame_stage;

pub use crate::app_interpolation_ext::AppInterpolationExt;
pub use crate::frame_interpolate::FrameInterpolate;
pub use crate::frame_interpolation_history::FrameInterpolationHistory;
pub use crate::frame_interpolation_plugin::FrameInterpolationPlugin;
pub use crate::frame_interpolation_systems::FrameInterpolationSystems;
pub use crate::frame_source::{FrameSamples, FrameSamplesFn, FrameSource};
pub use crate::history_component::HistoryComponent;
pub use crate::interpolation_bundle::{BundleShape, InterpolationBundle};
pub use crate::interpolation_config::InterpolationConfig;
pub use crate::interpolation_fn::InterpolationFn;
pub use crate::interpolation_fn::LerpFn;
pub use crate::interpolation_fns::InterpolationFns;
pub use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
pub use crate::interpolation_mode::InterpolationMode;
pub use crate::interpolation_plugin::InterpolationPlugin;
pub use crate::interpolation_registration_ext::InterpolationRegistrationExt;
pub use crate::interpolation_registry::InterpolationRegistry;
pub use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
pub use crate::interpolation_rule::InterpolationRule;
pub use crate::interpolation_sample_context::InterpolationSampleContext;
pub use crate::interpolation_sample_context::interpolation_fraction;
pub use crate::interpolation_state::InterpolationState;
pub use crate::interpolation_systems::InterpolationSystems;
pub use crate::interpolation_timeline::InterpolationTimeline;
pub use crate::into_interpolation_fn::{ComponentShape, IntoContextLerpFn, IntoLerpFn};
pub use crate::lerps::linear_lerp;
pub use crate::skip_frame_interpolation::SkipFrameInterpolation;
pub use crate::synced_interpolation_timeline::SyncedInterpolationTimeline;
