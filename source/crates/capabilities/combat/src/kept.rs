use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// Keeps a dead unit whose death a later stage has yet to answer, as the mode's `on_unit_death`
/// does when its budget is spent: the unit stays, dead, until that stage removes this, and then
/// decays at the end of the tick its `Decay` names, or of that tick once it passed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept;

impl SimComponent for Kept {
    const NAME: &'static str = "combat.kept";
    type Host = UnitEntity;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Kept {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
