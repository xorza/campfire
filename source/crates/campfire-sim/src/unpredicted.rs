use bevy_ecs::component::Component;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::world::World;

/// Marks a unit a world holds as its authority sent it, and does not simulate: on a predicting
/// client, every replicated unit its player does not control. It hides the unit from every query
/// that does not name it, so the sim runs only on what the world predicts; the rest holds the
/// server's state as it arrives. Collision names it, to part the predicted units from the held
/// ones, and presentation reads these units with `Allow<Unpredicted>`.
#[derive(Component, Debug)]
pub struct Unpredicted;

impl Unpredicted {
    /// Hides every unit with the marker from the queries of `world` that do not name it.
    pub fn register(world: &mut World) {
        let id = world.register_component::<Unpredicted>();
        world
            .resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(id);
    }
}
