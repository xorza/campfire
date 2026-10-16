use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Local, Query};
use campfire_players::{HeldOut, OutOfPlay};
use campfire_units::{Avatar, Dead, Owner, Player};

/// The mode's marks of who is out of play, on each player's entity (14-replication-audiences.md,
/// Prediction reads only what its client holds; 14-replication-audiences.md, A player out of play
/// sees what its team sees, or what a spectator sees).
#[derive(Debug)]
pub(crate) struct OutOfPlayMarks;

/// What a tick's end found of a player's avatars.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Avatars {
    held: bool,
    living: bool,
}

impl OutOfPlayMarks {
    /// Marks each player out of play, or no more, where that changed: one the mode's script holds
    /// out, or that holds avatars and every one is dead. A client holds the player entities
    /// unpredicted, which no query here reads, so only the server marks them.
    pub(crate) fn refresh(
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
