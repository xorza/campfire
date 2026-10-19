use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_stepper::Stepper;

/// The frames a server change takes to reach client 0 on a lossless link: one for the server to
/// send it and one for the client to apply it.
pub const PROPAGATION_FRAMES: usize = 2;

/// Runs `f` on the server's world.
pub fn on_server<S, R>(stepper: &mut Stepper<S>, f: impl FnOnce(&mut World) -> R) -> R {
    f(stepper.server_app_mut().world_mut())
}

/// Runs `f` on client 0's world.
pub fn on_client<S, R>(stepper: &mut Stepper<S>, f: impl FnOnce(&mut World) -> R) -> R {
    f(stepper.client_app_mut(0).world_mut())
}

pub fn spawn_on_server<S, B: Bundle>(stepper: &mut Stepper<S>, bundle: B) -> Entity {
    on_server(stepper, |world| world.spawn(bundle).id())
}

/// Client 0's copy of the server's `entity`, once replicated.
pub fn client_copy<S>(stepper: &Stepper<S>, entity: Entity) -> Option<Entity> {
    stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&entity)
        .copied()
}
