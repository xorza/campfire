use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::HostRule;

/// The kind of a unit an action delivers, a projectile or an area, which each requires, so a
/// spawn, a restore and a client's replication each give it with the delivery.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct DeliveryEntity;

impl HostRule for DeliveryEntity {
    fn holds(world: &World, entity: Entity) -> bool {
        world.get::<DeliveryEntity>(entity).is_some()
    }
}
