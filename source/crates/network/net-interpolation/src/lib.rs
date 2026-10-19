//! Interpolation of replicated components: a rule keeps a delayed history of a component's
//! received values and writes the value at the interpolation timeline, or smooths a predicted or
//! visual entity between fixed ticks.

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
#[cfg(feature = "internals")]
mod fixtures;
mod frame_history_fns;
mod frame_interpolation_plugin;
mod frame_interpolation_rules;
mod frame_interpolation_systems;
mod history_component;
mod history_receive;
mod history_seed;
mod interpolate;
mod interpolation_bundle;
mod interpolation_fn;
mod interpolation_fns;
mod interpolation_holders;
mod interpolation_marker_plugin;
mod interpolation_mode;
mod interpolation_plugin;
mod interpolation_plugins;
mod interpolation_registration_ext;
mod interpolation_registry;
mod interpolation_registry_plugin;
mod interpolation_rule;
mod interpolation_rule_component;
mod interpolation_rule_id;
mod interpolation_sample_cx;
mod interpolation_setup;
mod interpolation_stage;
mod interpolation_state;
mod interpolation_systems;
mod interpolation_timeline;
mod interpolation_timeline_plugin;
mod into_interpolation_fn;
mod lerps;
mod member_mode;
mod pipelines;
mod registered_rule;
mod rule_building;
mod rule_resolution;
mod skip_frame_interpolation;
mod synced_interpolation_timeline;
mod update_history_cx;

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
pub use crate::interpolation_fn::InterpolationFn;
pub use crate::interpolation_fn::LerpFn;
pub use crate::interpolation_fns::InterpolationFns;
pub use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
pub use crate::interpolation_mode::InterpolationMode;
pub use crate::interpolation_plugins::InterpolationPlugins;
pub use crate::interpolation_registration_ext::InterpolationRegistrationExt;
pub use crate::interpolation_registry::InterpolationRegistry;
pub use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
pub use crate::interpolation_rule::InterpolationRule;
pub use crate::interpolation_sample_cx::InterpolationSampleCx;
pub use crate::interpolation_sample_cx::interpolation_fraction;
pub use crate::interpolation_setup::InterpolationSetup;
pub use crate::interpolation_state::InterpolationState;
pub use crate::interpolation_systems::InterpolationSystems;
pub use crate::interpolation_timeline::InterpolationTimeline;
pub use crate::into_interpolation_fn::{ComponentShape, IntoCxLerpFn, IntoLerpFn};
pub use crate::lerps::linear_lerp;
pub use crate::skip_frame_interpolation::SkipFrameInterpolation;
pub use crate::synced_interpolation_timeline::SyncedInterpolationTimeline;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::interpolation_stack::{
        InterpolationStack, interpolation_timeline_is_synced,
    };
    pub use crate::fixtures::protocol::{CompCustomInterp, CompFull, InterpolationProtocolPlugin};
}
