use bevy_ecs::change_detection::{DetectChanges, Mut, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res};
use campfire_sim::{DueRun, Removals, SimTick, Unpredicted};

use crate::action_book::ActionBook;
use crate::action_slots::ActionSlots;
use crate::action_slots::slot_times::{SlotCharges, SlotTimes};

/// The holds of every action: its charges as each tick starts.
#[derive(Debug)]
pub(super) struct Holds;

impl Holds {
    /// Keeps each unit's charges as its slots stand, as each tick starts, after the orders that
    /// learn ranks: a slot that came to a rank of an action with charges fills, and each charge
    /// whose time came comes back. A slot's times change only when its charges do, as they
    /// replicate. A unit's charges follow from its slots, its times and the book, and from the
    /// tick only as a charge comes back: so a run visits the units whose slots or times changed
    /// since its last, those a predicting client predicts again, `returned`, whose parts kept
    /// their ticks while hidden, and those whose next charge `due` holds; every unit when the book
    /// changed.
    pub(super) fn hold_charges(
        actions: Res<'_, ActionBook>,
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, (Entity, Ref<'_, ActionSlots>, &mut SlotTimes)>,
        mut returned: Removals<'_, '_, Unpredicted>,
        (mut due, mut visits, mut charges): (
            Local<'_, DueRun>,
            Local<'_, Vec<Entity>>,
            Local<'_, Vec<(u8, Option<SlotCharges>)>>,
        ),
    ) {
        let now = tick.start();
        let every = actions.is_changed();
        if every {
            due.clear();
        }
        visits.clear();
        due.take_due(now, &mut visits);
        visits.extend(returned.read());
        visits.sort_unstable();
        visits.dedup();
        let mut charge = |entity, slots: &ActionSlots, times: &mut Mut<'_, SlotTimes>| {
            charges.clear();
            charges.extend(times.charges_due(slots, &actions, now));
            for &(slot, held) in &*charges {
                times.set_charges(slot, held);
            }
            if let Some(next) = times.next_charge(slots, &actions) {
                due.push(next, entity);
            }
        };
        for (entity, slots, mut times) in &mut units {
            let changed = every || slots.is_changed() || times.is_changed();
            if changed && visits.binary_search(&entity).is_err() {
                charge(entity, &slots, &mut times);
            }
        }
        for &entity in &*visits {
            if let Ok((_, slots, mut times)) = units.get_mut(entity) {
                charge(entity, &slots, &mut times);
            }
        }
    }
}
