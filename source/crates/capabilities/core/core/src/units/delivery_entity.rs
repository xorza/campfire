use bevy_ecs::component::Component;
use campfire_sim::KindMarker;

use crate::units::entity_kind::{EntityKind, KindAudience};
use crate::units::entity_kinds::EntityKinds;
use crate::units::unit_entity::UnitEntity;

/// The kind of a unit an action delivers, a projectile or an area, which each requires, so a
/// spawn, a restore and a client's replication each give it with the delivery.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable, on_add = EntityKinds::one)]
#[require(UnitEntity)]
pub struct DeliveryEntity;

impl KindMarker for DeliveryEntity {}

impl EntityKind for DeliveryEntity {
    const AUDIENCE: KindAudience = KindAudience::Seers;
    const OWNER_PREDICTS: bool = false;
}
