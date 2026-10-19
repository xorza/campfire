use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use serde::{Deserialize, Serialize};

use crate::player::Player;

/// Marks a player the mode's script holds out of play, `ctx.set_out`, as an eliminated or a
/// defeated player is, on its entity (10-replication-audiences.md, A player out of play sees what
/// its team sees, or what a spectator sees). Clients receive `OutOfPlay`, which it derives, not the
/// cause.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct HeldOut;

impl SimComponent for HeldOut {
    const NAME: &'static str = "players.held_out";
    type Host = Player;

    // A mark, which holds nothing.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for HeldOut {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
