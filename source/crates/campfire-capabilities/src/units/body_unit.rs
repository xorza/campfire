use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{KindMarker, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::entity_kind::{EntityKind, KindAudience};
use crate::units::entity_kinds::EntityKinds;
use crate::units::unit_entity::UnitEntity;

/// The kind of a unit with a body of its own, which a mode, a train or a test spawns: every unit
/// but a delivery. It is state, so a unit stays of its kind through a restore.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_add = EntityKinds::one)]
#[require(UnitEntity)]
pub struct BodyUnit;

impl KindMarker for BodyUnit {}

impl EntityKind for BodyUnit {
    const AUDIENCE: KindAudience = KindAudience::Seers;
    const OWNER_PREDICTS: bool = true;
}

impl SimComponent for BodyUnit {
    const NAME: &'static str = "units.body_unit";
    type Host = BodyUnit;

    // A unit of a named id would stand where a player's or the match's entity is looked for; and
    // an entity is of one kind.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<StableId>(entity)
            .is_some_and(|id| id.is_allocated())
            && EntityKinds::alone(world, entity)
    }
}

impl Replication for BodyUnit {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
