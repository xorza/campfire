use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::Player;
use serde::{Deserialize, Serialize};

/// Marks a player the mode's script holds out of play, `ctx.set_out`, as an eliminated or a
/// defeated player is, on its entity (design 14, D16). Clients receive `OutOfPlay`, which it
/// derives, not the cause.
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
