use bevy_ecs::entity::Entity;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_stepper::Stepper;

/// The frames a server change takes to reach a client on a lossless link: one for the server to
/// send it and one for the client to apply it.
pub const PROPAGATION_FRAMES: usize = 2;

/// What a stepper's clients hold of the server's replicated entities.
pub trait ClientCopies {
    /// Client `client`'s copy of the server's `entity`, once replicated.
    fn client_copy(&self, client: usize, entity: Entity) -> Option<Entity>;
}

impl<S> ClientCopies for Stepper<S> {
    fn client_copy(&self, client: usize, entity: Entity) -> Option<Entity> {
        let copies = self.client_world(client).resource::<ServerEntityMap>();
        copies.to_client().get(&entity).copied()
    }
}
