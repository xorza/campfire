use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::order_units::OrderUnits;
use crate::player::Player;

/// The units a player's orders command, on its entity, in increasing stable id, as its last
/// select left them (04-capabilities/control.md, A player's orders go to its commanded units). Its
/// player's prediction reads it, so it is sent to its player alone (10-replication-audiences.md,
/// The commanded units are their player's prediction).
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandedUnits(Vec<StableId>);

impl CommandedUnits {
    pub fn get(&self) -> &[StableId] {
        &self.0
    }

    /// Commands `units`, which rise, in place of what it commanded, in its own buffer.
    pub(crate) fn set(&mut self, units: &[StableId]) {
        debug_assert!(OrderUnits::rises(units), "commanded units rise");
        self.0.clear();
        self.0.extend_from_slice(units);
    }
}

impl SimComponent for CommandedUnits {
    const NAME: &'static str = "players.commanded_units";
    type Host = Player;

    // Its units rise, each once, as a select leaves them.
    fn check(&self, _: &World, _: Entity) -> bool {
        OrderUnits::rises(&self.0)
    }
}

impl Replication for CommandedUnits {
    const KIND: DataKind = DataKind::Commanded;
    type Sending = SentOnChange;
}
