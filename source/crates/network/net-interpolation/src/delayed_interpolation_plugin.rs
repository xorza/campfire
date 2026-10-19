use crate::interpolation_owners::InterpolationOwners;
use crate::interpolation_registry::InterpolationRegistry;
use crate::interpolation_stage::InterpolationStage;
use crate::interpolation_systems::InterpolationSystems;
use crate::pipelines::Pipelines;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::archetype::Archetypes;
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;

/// The stages of [`InterpolationSystems::Prepare`], where each rule and maintained component
/// installs its typed systems once the registry builds it, and the owners that resolve the
/// archetypes before each stage.
#[derive(Debug, Default)]
pub(crate) struct DelayedInterpolationPlugin;

impl Plugin for DelayedInterpolationPlugin {
    fn build(&self, app: &mut App) {
        Pipelines::mark(app, |pipelines| pipelines.delayed = true);
        app.init_resource::<InterpolationOwners>();
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
                Self::refresh_delayed_owners.in_set(InterpolationStage::RefreshHistory),
                Self::refresh_delayed_owners.in_set(InterpolationStage::RefreshApply),
            ),
        );
    }
}

impl DelayedInterpolationPlugin {}

impl DelayedInterpolationPlugin {
    /// Resolves the delayed interpolation of the archetypes created since the last refresh.
    fn refresh_delayed_owners(
        mut owners: ResMut<'_, InterpolationOwners>,
        archetypes: &Archetypes,
        components: &Components,
        registry: Res<'_, InterpolationRegistry>,
    ) {
        registry.refresh_owners(&mut owners.delayed, archetypes, components);
    }
}
