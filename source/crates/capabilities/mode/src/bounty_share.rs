use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{NotSent, Replication};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// A player's share of the cost of each unit its units kill that a death pays, 0 or more, which
/// starts at the mode's and a script raises, as a power does (04-capabilities/games.md, A kill's
/// bounty). State, on the player's entity, of a mode of a `[bounty]`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BountyShare(pub(crate) Num);

impl SimComponent for BountyShare {
    const NAME: &'static str = "mode.bounty_share";
    type Host = Player;

    // A share below 0 would take a resource from a killer.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0 >= Num::ZERO
    }
}

impl Replication for BountyShare {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
