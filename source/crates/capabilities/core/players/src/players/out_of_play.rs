use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Local, Query};
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_units::{Avatar, Dead, Owner, Player};
use serde::{Deserialize, Serialize};

use crate::players::held_out::HeldOut;

/// Marks a player out of play, on its entity (design 14, D16): while it holds avatars and every
/// one is dead, or while the mode's script holds it out. The server derives it as each tick ends
/// and sends it to every client, as who is out of play is the match's public state. A player out
/// of play that sees what a spectator sees gives no order, so the hash and the snapshot hold it:
/// a restore refuses the orders the run it restores refused.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutOfPlay;

/// What a tick's end found of a player's avatars.
#[derive(Debug, Clone, Copy, Default)]
pub struct Avatars {
    held: bool,
    living: bool,
}

impl OutOfPlay {
    /// Marks each player out of play, or no more, where that changed. A client holds the player
    /// entities unpredicted, which no query here reads, so only the server marks them.
    pub fn refresh(
        units: Query<'_, '_, (&Owner, Has<Dead>), With<Avatar>>,
        players: Query<'_, '_, (Entity, &Player, Has<HeldOut>, Has<OutOfPlay>)>,
        mut avatars: Local<'_, Vec<Avatars>>,
        mut commands: Commands<'_, '_>,
    ) {
        avatars.clear();
        for (owner, dead) in &units {
            let at = owner.slot().index();
            if avatars.len() <= at {
                avatars.resize(at + 1, Avatars::default());
            }
            avatars[at].held = true;
            avatars[at].living |= !dead;
        }
        for (entity, player, held_out, marked) in &players {
            let of = avatars
                .get(player.slot().index())
                .copied()
                .unwrap_or_default();
            let out = held_out || (of.held && !of.living);
            if out && !marked {
                commands.entity(entity).insert(OutOfPlay);
            } else if !out && marked {
                commands.entity(entity).remove::<OutOfPlay>();
            }
        }
    }
}

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
