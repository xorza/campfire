use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use campfire_actions::ActionBook;
use campfire_common::PlayerSlot;
use campfire_players::Player;
use campfire_sim::StableId;
use campfire_stats::PlayerModifiers;
use campfire_units::Owner;

use crate::holdings::{Holdings, OwnedUnits};
use crate::player_power::PlayerPower;
use crate::power_book::PowerBook;
use crate::queue_pace::QueuePace;
use crate::train_queue::{QueueIndex, TrainQueue};

/// The count of each player's power as each tick ends, on the server
/// (04-capabilities/production.md, The count runs as each tick ends).
#[derive(Debug)]
pub(crate) struct PowerCount;

impl PowerCount {
    /// Writes each player's power on its entity, where it changed, as the holdings follow the
    /// units; gives the mode's shortage modifier to each player that is short and takes it from
    /// each that is not, so the modifier holds as the count does whatever the snapshot a match
    /// restored from held (04-capabilities/production.md, A shortage is a player modifier); and, in
    /// a mode whose power slows production, times again the head of each queue of a player whose
    /// power changed, so one that passed its new end ends in the next tick
    /// (04-capabilities/production.md, A train's end follows the speed). Only the server counts it,
    /// which a client receives.
    pub(super) fn refresh(
        (book, actions): (Res<'_, PowerBook>, Res<'_, ActionBook>),
        mut units: ParamSet<
            '_,
            '_,
            (
                OwnedUnits<'_, '_>,
                Query<'_, '_, (Entity, &StableId, &Owner, &mut TrainQueue)>,
            ),
        >,
        mut holdings: ResMut<'_, Holdings>,
        mut players: Query<'_, '_, (&Player, &mut PlayerPower, Option<&mut PlayerModifiers>)>,
        (mut index, mut changed): (QueueIndex<'_>, Local<'_, Vec<(PlayerSlot, PlayerPower)>>),
    ) {
        let rules = book.0.expect("power is counted in a mode that counts it");
        holdings.follow(&units.p0());
        changed.clear();
        for (player, mut held, modifiers) in &mut players {
            let power = holdings.power(player.player_slot());
            if held.set_if_neq(power) {
                changed.push((player.player_slot(), power));
            }
            let (Some(short), Some(mut modifiers)) = (rules.short, modifiers) else {
                continue;
            };
            match (power.is_short(), modifiers.holds(short)) {
                (true, false) => modifiers.add(short),
                (false, true) => modifiers.remove(short),
                (true, true) | (false, false) => {}
            }
        }
        if rules.speed.is_none() || changed.is_empty() {
            return;
        }
        changed.sort_unstable_by_key(|&(player_slot, _)| player_slot);
        for (entity, &id, owner, mut queue) in &mut units.p1() {
            let found = changed.binary_search_by_key(&owner.player_slot(), |&(slot, _)| slot);
            let Ok(at) = found else {
                continue;
            };
            let pace = QueuePace::of(*book, &actions, Some(changed[at].1));
            if let Some(head) = queue.repaced(pace) {
                queue.retime(head, &mut index.of(id, entity));
            }
        }
    }
}
