use bevy_ecs::change_detection::{DetectChanges, Mut, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, SimTick, StableId, TimedKept, TimedProcess, TimedRuns};

use crate::action_book::ActionBook;
use crate::action_slots::ActionSlots;
use crate::action_slots::slot_times::{SlotCharges, SlotTimes};

/// The holds of every action: its charges as each tick starts.
#[derive(Debug)]
pub(super) struct Holds;

/// The charges of a unit's slots, which come back one at a time as their times come: a unit is
/// due at the time its next charge comes back, as the book's charges of its slots' actions say.
#[derive(Debug)]
pub(super) struct Charges;

impl TimedProcess for Charges {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::FollowsChanges;
    type Parts = (&'static ActionSlots, &'static SlotTimes);
    type Books = Res<'static, ActionBook>;

    fn due(book: &Res<'_, ActionBook>, (slots, times): (&ActionSlots, &SlotTimes)) -> Option<Tick> {
        times.next_charge(slots, book)
    }
}

impl Holds {
    /// Keeps each unit's charges as its slots stand, as each tick starts, after the orders that
    /// learn ranks: a slot that came to a rank of an action with charges fills, and each charge
    /// whose time came comes back. A slot's times change only when its charges do, as they
    /// replicate. A unit's charges follow from its slots, its times and the book, and from the
    /// tick only as a charge comes back: so a run visits the units whose slots or times changed
    /// since its last, as a unit a predicting client predicts again counts its parts changed, and
    /// those the index of the charges holds due, and pushes each it visits at its next charge's
    /// time; every unit when the book changed.
    pub(super) fn hold_charges(
        actions: Res<'_, ActionBook>,
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, (Entity, &StableId, Ref<'_, ActionSlots>, &mut SlotTimes)>,
        mut index: ResMut<'_, DueIndex<Charges>>,
        (mut due, mut visits, mut charges): (
            Local<'_, Vec<DueUnit>>,
            Local<'_, Vec<Entity>>,
            Local<'_, Vec<(u8, Option<SlotCharges>)>>,
        ),
    ) {
        let now = tick.start();
        let every = actions.is_changed();
        due.clear();
        index.take_due(now, &mut due);
        visits.clear();
        visits.extend(due.iter().map(|unit| unit.entity));
        visits.sort_unstable();
        visits.dedup();
        let mut charge = |entity, id, slots: &ActionSlots, times: &mut Mut<'_, SlotTimes>| {
            charges.clear();
            charges.extend(times.charges_due(slots, &actions, now));
            for &(slot, held) in &*charges {
                times.set_charges(slot, held);
            }
            if let Some(next) = times.next_charge(slots, &actions) {
                index.push(next, id, entity);
            }
        };
        for (entity, &id, slots, mut times) in &mut units {
            let changed = every || slots.is_changed() || times.is_changed();
            if changed && visits.binary_search(&entity).is_err() {
                charge(entity, id, &slots, &mut times);
            }
        }
        for &entity in &*visits {
            if let Ok((_, &id, slots, mut times)) = units.get_mut(entity) {
                charge(entity, id, &slots, &mut times);
            }
        }
    }
}
