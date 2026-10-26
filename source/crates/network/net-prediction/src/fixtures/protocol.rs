use crate::prediction_registration_ext::PredictionRegistrationExt;
use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_frame_interpolation::{
    AppFrameInterpolationExt, FrameInterpolationPlugin, FrameRule, InterpolationSampleCx,
};
use campfire_net_interpolation::InterpolationFns;
use campfire_net_interpolation::InterpolationRegistrationExt;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::AppComponentExt;
use campfire_net_replication::internals::{CompMap, CompRepliconDiff};
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompPredictionOnly(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompNotNetworked(pub f32);

/// Lerps `$name`, a newtype of one `f32` with a default, linearly, and diffs it by subtraction, as
/// correction needs.
macro_rules! linear_correction {
    ($name:ident) => {
        impl ::bevy_curve::Ease for $name {
            fn interpolating_curve_unbounded(
                start: Self,
                end: Self,
            ) -> impl ::bevy_curve::Curve<Self> {
                ::bevy_curve::FunctionCurve::new(::bevy_curve::Interval::UNIT, move |t| {
                    Self(start.0 + (end.0 - start.0) * t)
                })
            }
        }

        impl ::campfire_net_replication::Diffable for $name {
            fn base_value() -> Self {
                Self::default()
            }

            fn diff(&self, other: &Self) -> Self {
                Self(other.0 - self.0)
            }

            fn apply_diff(&mut self, delta: &Self) {
                self.0 += delta.0;
            }
        }
    };
}

#[cfg(test)]
pub(crate) use linear_correction;

#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Reflect)]
pub struct CompCorr(pub f32);
linear_correction!(CompCorr);

#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Reflect)]
pub struct ShownCorr(pub f32);
linear_correction!(ShownCorr);

/// The visual of `CompCorr`: its linear blend between the last two ticks.
pub fn show_corr(start: &CompCorr, end: &CompCorr, cx: InterpolationSampleCx) -> ShownCorr {
    ShownCorr(start.0 + (end.0 - start.0) * cx.t)
}

/// The prediction layer's test protocol: predicted, corrected and local-rollback components.
#[derive(Debug)]
pub struct PredictionProtocolPlugin;

impl Plugin for PredictionProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<CompFull>()
            .already_replicated()
            .predict();
        app.register_component::<CompPredictionOnly>()
            .replicate()
            .predict();
        app.register_component::<CompCorr>()
            .replicate()
            .predict()
            .correct_linear::<ShownCorr, ShownCorr>()
            .interpolate(InterpolationFns::linear());
        app.register_frame_rule::<CompCorr, ShownCorr, ()>(FrameRule::new(show_corr));
        app.register_component::<CompMap>()
            .already_replicated()
            .predict();
        app.register_component::<CompNotNetworked>()
            .local_rollback();
        app.register_component::<CompRepliconDiff>()
            .already_replicated_diff()
            .predict();

        if !app.is_plugin_added::<FrameInterpolationPlugin>() {
            app.add_plugins(FrameInterpolationPlugin);
        }
    }
}
