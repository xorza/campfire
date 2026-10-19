use bevy_app::{App, Plugin};
use bevy_curve::{Curve, Ease, FunctionCurve, Interval};
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use campfire_net_replication::AppComponentExt;
use campfire_net_replication::internals::CompRepliconDiff;
use serde::{Deserialize, Serialize};

use crate::interpolation_fns::InterpolationFns;
use crate::interpolation_registration_ext::InterpolationRegistrationExt;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompFull(pub f32);

impl Ease for CompFull {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            Self(start.0 + (end.0 - start.0) * t)
        })
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct CompCustomInterp(pub f32);

/// The interpolation layer's test protocol: linear, custom and diff interpolation.
#[derive(Debug)]
pub struct InterpolationProtocolPlugin;

impl Plugin for InterpolationProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<CompFull>()
            .replicate()
            .interpolate(InterpolationFns::linear());
        app.register_component::<CompCustomInterp>()
            .replicate()
            .interpolate(InterpolationFns::history_only());
        app.register_component::<CompRepliconDiff>()
            .already_replicated_diff()
            .interpolate(InterpolationFns::history_only());
    }
}
