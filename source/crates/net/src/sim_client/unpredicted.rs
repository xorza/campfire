use bevy_ecs::component::Component;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_ecs::world::World;
use lightyear::prelude::{Predicted, Replicated};

/// Marks a replicated unit the client does not predict, and hides it from every query that does
/// not name it: the client's sim runs only on what its player controls, and every other unit
/// holds the server's state as it arrives. Presentation reads these units with
/// `Allow<Unpredicted>`.
#[derive(Component, Debug)]
pub struct Unpredicted;

impl Unpredicted {
    /// Marks each replicated entity until it is predicted, in whatever order the two markers
    /// arrive.
    pub(crate) fn install(world: &mut World) {
        let id = world.register_component::<Unpredicted>();
        world
            .resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(id);
        world.add_observer(
            |added: On<'_, '_, Add, Replicated>,
             predicted: Query<'_, '_, (), With<Predicted>>,
             mut commands: Commands<'_, '_>| {
                if !predicted.contains(added.entity) {
                    commands.entity(added.entity).insert(Unpredicted);
                }
            },
        );
        world.add_observer(
            |added: On<'_, '_, Add, Predicted>, mut commands: Commands<'_, '_>| {
                commands.entity(added.entity).remove::<Unpredicted>();
            },
        );
    }
}
