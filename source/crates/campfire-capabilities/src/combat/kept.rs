use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// Keeps a dead unit whose death a later stage has yet to answer, as the mode's `on_unit_died`
/// does when its pool is spent: the unit stays, dead, until that stage removes this, and then
/// despawns at the end of that tick if its type despawns.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Kept;

impl SimComponent for Kept {
    const NAME: &'static str = "combat.kept";

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Kept {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
