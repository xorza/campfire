use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::players::held_out::HeldOut;
use crate::players::player::Player;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnChange;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::owner::Owner;
use crate::units::tag_book::TagBook;
use crate::units::unit_type::UnitType;

/// Marks a player out of play, on its entity (design 14, D16): while it holds avatars and every
/// one is dead, or while the mode's script holds it out. The server derives it as each tick ends
/// and sends it to every client, as who is out of play is the match's public state; no hash or
/// snapshot holds it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutOfPlay;

/// What a tick's end found of a player's avatars.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Avatars {
    held: bool,
    living: bool,
}

impl OutOfPlay {
    /// Marks each player out of play, or no more, where that changed. A client holds the player
    /// entities unpredicted, which no query here reads, so only the server marks them.
    pub(crate) fn refresh(
        book: Option<Res<'_, TagBook>>,
        units: Query<'_, '_, (&Owner, &UnitType, Has<Dead>)>,
        players: Query<'_, '_, (Entity, &Player, Has<HeldOut>, Has<OutOfPlay>)>,
        mut avatars: Local<'_, Vec<Avatars>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(book) = book else {
            return;
        };
        let avatar = EngineTag::Avatar.tag();
        avatars.clear();
        for (owner, &unit_type, dead) in &units {
            if !book.own(unit_type).contains(avatar) {
                continue;
            }
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

    // Derived, so no snapshot holds one for a restore to check.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for OutOfPlay {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnChange;
}
