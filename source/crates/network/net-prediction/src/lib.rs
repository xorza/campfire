//! Client-side prediction: the predicted history of each component, the rollback from a confirmed
//! tick whose state differs, and the correction that eases the visual error.

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod correction;
mod correction_ease;
mod correction_policy;
mod disable_rollback;
mod disabled_during_rollback;
#[cfg(feature = "internals")]
mod fixtures;
mod history_receive;
mod locally_predicted;
mod mismatch_fn;
mod misprediction;
mod on_timeline;
mod pending_switch;
mod predicted_component_registration;
mod prediction_counts;
mod prediction_despawn_command;
mod prediction_despawn_commands_ext;
mod prediction_despawned;
mod prediction_diagnostics_plugin;
mod prediction_gate;
mod prediction_history;
mod prediction_marker_plugin;
mod prediction_mode;
mod prediction_plugin;
mod prediction_plugins;
mod prediction_registration;
mod prediction_registration_ext;
mod prediction_registry;
mod prediction_registry_plugin;
mod prediction_setup;
mod prediction_systems;
mod previous_visual;
mod rollback_check_stage;
mod rollback_check_state;
mod rollback_mode;
mod rollback_plugin;
mod rollback_scan;
mod rollback_setup;
mod rollback_systems;
mod switch_blend;
mod switch_direction;
mod tick_period_history;
mod timeline_switch;
mod timeline_switch_plugin;
mod timeline_switch_setup;
mod visual_correction;

pub use crate::correction_ease::CorrectionEase;
pub use crate::correction_policy::CorrectionPolicy;
pub use crate::disable_rollback::DisableRollback;
pub use crate::disabled_during_rollback::DisabledDuringRollback;
pub use crate::locally_predicted::LocallyPredicted;
pub use crate::mismatch_fn::MismatchFn;
pub use crate::predicted_component_registration::PredictedComponentRegistration;
pub use crate::prediction_counts::PredictionCounts;
pub use crate::prediction_despawn_command::PredictionDespawnCommand;
pub use crate::prediction_despawn_commands_ext::PredictionDespawnCommandsExt;
pub use crate::prediction_despawned::PredictionDespawned;
pub use crate::prediction_diagnostics_plugin::PredictionDiagnosticsPlugin;
pub use crate::prediction_history::{Iter, PredictionHistory};
pub use crate::prediction_marker_plugin::PredictionMarkerPlugin;
pub use crate::prediction_mode::PredictionMode;
pub use crate::prediction_plugins::PredictionPlugins;
pub use crate::prediction_registration_ext::LocalRollbackMode;
pub use crate::prediction_registration_ext::PredictionRegistrationExt;
pub use crate::prediction_registry::PredictionRegistry;
pub use crate::prediction_registry_plugin::PredictionRegistryPlugin;
pub use crate::prediction_setup::PredictionSetup;
pub use crate::prediction_systems::PredictionSystems;
pub use crate::previous_visual::PreviousVisual;
pub use crate::rollback_check_state::RollbackCheckState;
pub use crate::rollback_mode::RollbackMode;
pub use crate::rollback_plugin::RollbackPlugin;
pub use crate::rollback_setup::RollbackSetup;
pub use crate::rollback_systems::RollbackSystems;
pub use crate::switch_blend::SwitchBlend;
pub use crate::switch_direction::SwitchDirection;
pub use crate::timeline_switch::TimelineSwitch;
pub use crate::timeline_switch_setup::TimelineSwitchSetup;
pub use crate::visual_correction::{VisualCorrection, VisualOffset};

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::prediction_stack::PredictionStack;
    pub use crate::fixtures::protocol::{
        CompCorr, CompCorrectionBundleA, CompCorrectionBundleB, CompMixedCorrectionBundleA,
        CompMixedCorrectionBundleB, CompNotNetworked, CompPredictionOnly, PredictionProtocolPlugin,
        correction_bundle_lerp, mixed_correction_bundle_lerp,
    };
}
