use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{ActionBook, ActionSlots, SlotTimes};
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, StableId, TickClock, TimedKept, TimedProcess, TimedRuns};

use crate::grants::{Grant, GrantFrom, Grants};
use crate::lifetime::Hold;
use crate::modifier_book::ModifierBook;
use crate::modifier_params::ModifierParams;
use crate::modifiers::Modifiers;
use crate::stat_book::StatBook;

/// The grants of the actions' passives and holds, which stats pushes for each action a unit's
/// slots hold, as each stage that changes ranks or cooldowns ends.
#[derive(Debug)]
pub(crate) struct ActionPassives;

/// A unit's passives that hold only off cooldown: a unit is due as the first of their cooldowns
/// ends, when its passive's grant comes back.
impl TimedProcess for ActionPassives {
    const RUNS: TimedRuns = TimedRuns::EveryRole;
    const KEPT: TimedKept = TimedKept::FollowsChanges;
    type Parts = (&'static ActionSlots, &'static SlotTimes);
    type Books = Res<'static, ActionBook>;

    fn due(book: &Res<'_, ActionBook>, (slots, times): (&ActionSlots, &SlotTimes)) -> Option<Tick> {
        let slots = slots.iter().zip(times.iter());
        let waits = slots.filter_map(|(slot, time)| {
            let action = book.get(slot.action?)?;
            let waits = action.passive.is_some_and(|passive| passive.while_ready);
            (waits && slot.rank.is_some()).then_some(time.ready_at)
        });
        waits.min()
    }
}

impl ActionPassives {
    /// Pushes the grants of each unit's passives and holds as its slots stand: the passive of each
    /// action with a rank, and with `passive_while_ready` off cooldown, and the `hold` of each
    /// toggle that is on, each from the unit itself at the action's rank; and none of each other.
    /// What a unit wants follows from its slots, its modifiers and the books, and from the tick
    /// only as a cooldown a passive waits for ends: so a run visits only the units whose slots or
    /// modifiers changed since its last, and those the index of the waits holds due, in a buffer
    /// it keeps, unless a book changed; it pushes each unit whose passive waits at the wait's end.
    pub(crate) fn want(
        (actions, book, stats): (
            Res<'_, ActionBook>,
            Res<'_, ModifierBook>,
            Res<'_, StatBook>,
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
        mut index: ResMut<'_, DueIndex<Self>>,
        (mut due, mut visits): (Local<'_, Vec<DueUnit>>, Local<'_, Vec<Entity>>),
    ) {
        let every =
            actions.is_changed() || book.is_changed() || stats.is_changed() || params.changed();
        let now = clock.start();
        due.clear();
        index.take_due(now, &mut due);
        visits.clear();
        visits.extend(due.iter().map(|unit| unit.entity));
        visits.sort_unstable();
        for (entity, &id, slots, times, modifiers) in &units {
            let changed = slots.is_changed() || times.is_changed() || modifiers.is_changed();
            if !(every || changed || visits.binary_search(&entity).is_ok()) {
                continue;
            }
            grants.unit(entity, id);
            for (slot, time) in slots.iter().zip(times.iter()) {
                let Some(action_id) = slot.action else {
                    continue;
                };
                let action = actions
                    .get(action_id)
                    .expect("a slot's action is in the book");
                if let Some(passive) = action.passive {
                    let cooling = passive.while_ready && time.ready_at > now;
                    if slot.rank.is_some() && cooling {
                        index.push(time.ready_at, id, entity);
                    }
                    grants.push(Grant {
                        modifier: passive.modifier,
                        from: GrantFrom::Itself,
                        hold: Hold::Passive,
                        action: Some(action_id),
                        rank: slot.rank.filter(|_| !cooling),
                    });
                }
                // A channel's hold its channel holds each tick it runs, as its progress, not its
                // slots, changes.
                let toggles = action.ranks.iter().any(|rank| rank.toggle.is_some());
                if let Some(hold) = action.hold.filter(|_| toggles) {
                    grants.push(Grant {
                        modifier: hold,
                        from: GrantFrom::Itself,
                        hold: Hold::Running,
                        action: Some(action_id),
                        rank: slot.rank.filter(|_| time.toggle.is_some()),
                    });
                }
            }
        }
    }
}
