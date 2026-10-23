//! Delayed interpolation of replicated components: a rule keeps a history of a component's received
//! values and writes the value at the interpolation timeline.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod app_interpolation_ext;
mod confirmed_history_fns;
#[cfg(feature = "internals")]
mod fixtures;
mod history_receive;
mod history_seed;
mod interpolate;
mod interpolation_bundle;
mod interpolation_fn;
mod interpolation_fns;
mod interpolation_holders;
mod interpolation_marker_plugin;
mod interpolation_mode;
mod interpolation_phase_systems;
mod interpolation_plugin;
mod interpolation_plugins;
mod interpolation_registration_ext;
mod interpolation_registry;
mod interpolation_registry_plugin;
mod interpolation_rule;
mod interpolation_rule_component;
mod interpolation_rule_id;
mod interpolation_setup;
mod interpolation_state;
mod interpolation_systems;
mod interpolation_timeline;
mod interpolation_timeline_plugin;
mod into_interpolation_fn;
mod member_mode;
mod pending_diffs;
mod registered_rule;
mod rule_building;
mod rule_resolution;
mod sample_ticks;

pub use crate::app_interpolation_ext::AppInterpolationExt;
pub use crate::interpolation_bundle::{BundleShape, InterpolationBundle};
pub use crate::interpolation_fn::InterpolationFn;
pub use crate::interpolation_fns::InterpolationFns;
pub use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
pub use crate::interpolation_mode::InterpolationMode;
pub use crate::interpolation_plugins::InterpolationPlugins;
pub use crate::interpolation_registration_ext::InterpolationRegistrationExt;
pub use crate::interpolation_registry::InterpolationRegistry;
pub use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
pub use crate::interpolation_rule::InterpolationRule;
pub use crate::interpolation_setup::InterpolationSetup;
pub use crate::interpolation_state::InterpolationState;
pub use crate::interpolation_systems::InterpolationSystems;
pub use crate::interpolation_timeline::InterpolationTimeline;
pub use crate::into_interpolation_fn::{ComponentShape, IntoCxLerpFn, IntoLerpFn};

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::interpolation_stack::{
        InterpolationStack, interpolation_timeline_is_synced,
    };
    pub use crate::fixtures::protocol::{CompCustomInterp, CompFull, InterpolationProtocolPlugin};
}
