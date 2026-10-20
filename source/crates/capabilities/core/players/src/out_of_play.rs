use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::player::Player;

/// Marks a player out of play, on its entity (10-replication-audiences.md, A player out of play
/// sees what its team sees, or what a spectator sees): while it holds avatars and every one is
/// dead, or while the mode's script holds it out. The mode's server derives it as each tick ends,
/// as out of play is the mode's (10-replication-audiences.md, Prediction reads only what its client
/// holds), and sends it to every client, as who is out of play is the match's public state. A
/// player out of play that sees what a spectator sees gives no order, so the hash and the snapshot
/// hold it: a restore refuses the orders the run it restores refused.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutOfPlay;

impl SimComponent for OutOfPlay {
    const NAME: &'static str = "players.out_of_play";
    type Host = Player;

    // A mark, which holds nothing; the tick's end derives it again.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for OutOfPlay {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnChange;
}
