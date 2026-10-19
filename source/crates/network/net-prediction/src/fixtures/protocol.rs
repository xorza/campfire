use crate::prediction_registration_ext::PredictionRegistrationExt;
use bevy_app::{App, Plugin};
use bevy_curve::{Curve, Ease, FunctionCurve, Interval};
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_interpolation::AppInterpolationExt;
use campfire_net_interpolation::FrameInterpolationPlugin;
use campfire_net_interpolation::InterpolationFns;
use campfire_net_interpolation::InterpolationRegistrationExt;
use campfire_net_interpolation::InterpolationRule;
use campfire_net_interpolation::InterpolationSampleContext;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::AppComponentExt;
use campfire_net_replication::Diffable;
use campfire_net_replication::internals::{CompMap, CompRepliconDiff};
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompPredictionOnly(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompNotNetworked(pub f32);

/// A component that lerps linearly and diffs by subtraction, as correction needs.
macro_rules! correction_component {
    ($name:ident) => {
        #[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Reflect)]
        pub struct $name(pub f32);

        impl Ease for $name {
            fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
                FunctionCurve::new(Interval::UNIT, move |t| {
                    Self(start.0 + (end.0 - start.0) * t)
                })
            }
        }

        impl Diffable for $name {
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

correction_component!(CompCorr);
correction_component!(CompCorrectionBundleA);
correction_component!(CompCorrectionBundleB);
correction_component!(CompMixedCorrectionBundleA);
correction_component!(CompMixedCorrectionBundleB);

/// Offsets each member and adds the sample's interval, so a test tells the bundle's output from
/// each member's own.
pub fn correction_bundle_lerp(
    start: (&CompCorrectionBundleA, &CompCorrectionBundleB),
    end: (&CompCorrectionBundleA, &CompCorrectionBundleB),
    context: InterpolationSampleContext,
) -> (CompCorrectionBundleA, CompCorrectionBundleB) {
    let sample_delta_secs = context.sample_delta_secs.unwrap_or_default();
    (
        CompCorrectionBundleA(
            100.0 + start.0.0 + (end.0.0 - start.0.0) * context.t + sample_delta_secs,
        ),
        CompCorrectionBundleB(
            200.0 + start.1.0 + (end.1.0 - start.1.0) * context.t + sample_delta_secs,
        ),
    )
}

/// Writes the sum into the corrected member, and a constant into the other, so a test sees a
/// correction that fails to restore the bundle's output.
pub fn mixed_correction_bundle_lerp(
    start: (&CompMixedCorrectionBundleA, &CompMixedCorrectionBundleB),
    end: (&CompMixedCorrectionBundleA, &CompMixedCorrectionBundleB),
    context: InterpolationSampleContext,
) -> (CompMixedCorrectionBundleA, CompMixedCorrectionBundleB) {
    let a = start.0.0 + (end.0.0 - start.0.0) * context.t;
    let b = start.1.0 + (end.1.0 - start.1.0) * context.t;
    (
        CompMixedCorrectionBundleA(a + b + context.sample_delta_secs.unwrap_or_default()),
        CompMixedCorrectionBundleB(10_000.0),
    )
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
        app.register_component::<CompCorrectionBundleA>()
            .replicate()
            .predict()
            .correct_linear::<CompCorrectionBundleA>();
        app.register_component::<CompCorrectionBundleB>()
            .replicate()
            .predict()
            .correct_linear::<CompCorrectionBundleB>();
        app.register_interpolation_rule::<CompCorrectionBundleA, ()>(InterpolationRule::new(
            InterpolationFns::no_history(
                |_: &CompCorrectionBundleA, _: &CompCorrectionBundleA, _: f32| {
                    CompCorrectionBundleA(1_000.0)
                },
            ),
        ));
        app.register_interpolation_rule::<CompCorrectionBundleB, ()>(InterpolationRule::new(
            InterpolationFns::no_history(
                |_: &CompCorrectionBundleB, _: &CompCorrectionBundleB, _: f32| {
                    CompCorrectionBundleB(2_000.0)
                },
            ),
        ));
        app.register_bundle_interpolation_rule::<(CompCorrectionBundleA, CompCorrectionBundleB), ()>(InterpolationRule::new(InterpolationFns::no_history_with_context(correction_bundle_lerp)));
        app.register_component::<CompMixedCorrectionBundleA>()
            .replicate()
            .predict()
            .correct_linear::<CompMixedCorrectionBundleA>();
        app.register_component::<CompMixedCorrectionBundleB>()
            .replicate()
            .predict();
        app.register_interpolation_rule::<CompMixedCorrectionBundleA, ()>(InterpolationRule::new(
            InterpolationFns::no_history(
                |_: &CompMixedCorrectionBundleA, _: &CompMixedCorrectionBundleA, _: f32| {
                    CompMixedCorrectionBundleA(1_000.0)
                },
            ),
        ));
        app.register_interpolation_rule::<CompMixedCorrectionBundleB, ()>(InterpolationRule::new(
            InterpolationFns::no_history(
                |_: &CompMixedCorrectionBundleB, _: &CompMixedCorrectionBundleB, _: f32| {
                    CompMixedCorrectionBundleB(2_000.0)
                },
            ),
        ));
        app.register_bundle_interpolation_rule::<(CompMixedCorrectionBundleA, CompMixedCorrectionBundleB), ()>(InterpolationRule::new(InterpolationFns::no_history_with_context(mixed_correction_bundle_lerp)));
        app.register_component::<CompCorr>()
            .replicate()
            .predict()
            .correct_linear::<CompCorr>()
            .interpolate(InterpolationFns::linear());
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
