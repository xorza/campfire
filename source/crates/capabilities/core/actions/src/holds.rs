use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{SimTick, StableId, TickClock};
use campfire_stats::{Grant, Grants, Hold, ModifierBook, ModifierParams, Modifiers, StatBook};

use crate::action_book::ActionBook;
use crate::action_slots::ActionSlots;
use crate::action_slots::slot_times::{SlotCharges, SlotTimes};
use crate::ready_waits::ReadyWaits;

/// The holds of every action: its charges as each tick starts, and the grants of its passives
/// then and as each stage that changes ranks or cooldowns ends.
#[derive(Debug)]
pub(super) struct Holds;

impl Holds {
    /// Keeps each unit's charges as its slots stand, as each tick starts, after the orders that
    /// learn ranks: a slot that came to a rank of an action with charges fills, and each charge
    /// whose time came comes back. A slot's times change only when its charges do, as they
    /// replicate.
    pub(super) fn hold_charges(
        actions: Res<'_, ActionBook>,
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, (&ActionSlots, &mut SlotTimes)>,
        mut due: Local<'_, Vec<(u8, Option<SlotCharges>)>>,
    ) {
        let now = tick.start();
        for (slots, mut times) in &mut units {
            due.clear();
            due.extend(times.charges_due(slots, &actions, now));
            for &(slot, charges) in &*due {
                times.set_charges(slot, charges);
            }
        }
    }

    /// Pushes the grants of each unit's passives and holds as its slots stand: the passive of each
    /// action with a rank, and with `passive_while_ready` off cooldown, and the `hold` of each
    /// action whose toggle is on or whose channel runs, each from the unit itself at the action's
    /// rank; and none of each other. What a unit wants follows from its slots, its modifiers and
    /// the books, and from the tick only for a passive that waits for its action's cooldown: so a
    /// run visits only the units whose slots or modifiers changed since its last, and those that
    /// wait, unless a book changed.
    pub(super) fn want_passives(
        (actions, book, stats): (
            Res<'_, ActionBook>,
            Option<Res<'_, ModifierBook>>,
            Option<Res<'_, StatBook>>,
        ),
        clock: TickClock<'_>,
        params: ModifierParams<'_, '_>,
        mut grants: ResMut<'_, Grants>,
        units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                Ref<'_, ActionSlots>,
                Ref<'_, SlotTimes>,
                Ref<'_, Modifiers>,
            ),
        >,
        mut waiting: Local<'_, ReadyWaits>,
    ) {
        let Some(stats) = stats else {
            return;
        };
        let Some(book) = book else {
            return;
        };
        let every =
            actions.is_changed() || book.is_changed() || stats.is_changed() || params.changed();
        let now = clock.start();
        for (entity, &id, slots, times, modifiers) in &units {
            let changed = slots.is_changed() || times.is_changed() || modifiers.is_changed();
            if !(every || changed || waiting.waits(entity)) {
                continue;
            }
            let mut waits = false;
            grants.unit(entity, id);
            for ((index, slot), time) in slots.indexed().zip(times.iter()) {
                let Some(action_id) = slot.action else {
                    continue;
                };
                let action = actions
                    .get(action_id)
                    .expect("a slot's action is in the book");
                if let Some(passive) = action.passive {
                    let cooling = passive.while_ready && time.ready_at > now;
                    waits |= slot.rank.is_some() && cooling;
                    grants.push(Grant {
                        modifier: passive.modifier,
                        hold: Hold::Passive,
                        action: Some(action_id),
                        rank: slot.rank.filter(|_| !cooling),
                    });
                }
                if let Some(hold) = action.hold {
                    let runs = time.toggle.is_some() || slots.channeling() == Some(index);
                    grants.push(Grant {
                        modifier: hold,
                        hold: Hold::Running,
                        action: Some(action_id),
                        rank: slot.rank.filter(|_| runs),
                    });
                }
            }
            waiting.set(entity, waits);
        }
    }
}
