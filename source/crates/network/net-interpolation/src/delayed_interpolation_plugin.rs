use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_registry::InterpolationRegistry;
use crate::interpolation_stage::InterpolationStage;
use crate::interpolation_systems::InterpolationSystems;
use crate::pipelines::Pipelines;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::archetype::Archetypes;
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;

/// The stages of [`InterpolationSystems::Prepare`], where each rule and maintained component
/// installs its typed systems once the registry builds it, and the holders that resolve the
/// archetypes before each stage.
#[derive(Debug, Default)]
pub(crate) struct DelayedInterpolationPlugin;

impl Plugin for DelayedInterpolationPlugin {
    fn build(&self, app: &mut App) {
        Pipelines::mark(app, |pipelines| pipelines.delayed = true);
        app.init_resource::<InterpolationHolders>();
        app.configure_sets(
            Update,
            (
                InterpolationStage::RefreshHistory,
                InterpolationStage::History,
                InterpolationStage::RefreshApply,
                InterpolationStage::Apply,
            )
                .chain()
                .in_set(InterpolationSystems::Prepare),
        );
        app.add_systems(
            Update,
            (
                Self::refresh_delayed_holders.in_set(InterpolationStage::RefreshHistory),
                Self::refresh_delayed_holders.in_set(InterpolationStage::RefreshApply),
            ),
        );
    }
}

impl DelayedInterpolationPlugin {}

impl DelayedInterpolationPlugin {
    /// Resolves the delayed interpolation of the archetypes created since the last refresh.
    fn refresh_delayed_holders(
        mut holders: ResMut<'_, InterpolationHolders>,
        archetypes: &Archetypes,
        components: &Components,
        registry: Res<'_, InterpolationRegistry>,
    ) {
        registry.refresh_holders(&mut holders.delayed, archetypes, components);
    }
}
