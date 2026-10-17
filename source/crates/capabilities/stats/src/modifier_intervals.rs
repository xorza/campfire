use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState};
use bevy_ecs::system::{Local, SystemState};
use bevy_ecs::world::World;
use campfire_script::{Hook, ScriptsWorld};
use campfire_sim::{Removals, SimTick, StableId};
use campfire_units::{Dead, TagBook, TagEffect};
use campfire_values::{ModifierId, TagSet, UnitTags};

use crate::carried_mut::CarriedMut;
use crate::modifier_book::ModifierBook;
use crate::modifier_events::ModifierEvents;
use crate::modifier_hooks::Heard;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The intervals of the modifiers units carry, which run their hooks in Hit.
#[derive(Debug)]
pub struct ModifierIntervals;

/// An instance whose interval comes this tick: its carrier, its modifier and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct IntervalDue {
    carrier: StableId,
    id: ModifierId,
    source: Option<StableId>,
}

/// The carriers whose intervals a run counts: each unit that holds modifiers.
type Carriers = QueryState<(
    &'static StableId,
    &'static mut Modifiers,
    &'static mut ModifierClocks,
    Option<&'static UnitTags>,
    Has<Dead>,
)>;

impl ModifierIntervals {
    /// Counts each carrier's intervals, and runs `on_interval` of each instance whose interval
    /// comes this tick, by carrier's stable id, then modifier, then source. A dead carrier's
    /// intervals keep the clock and run nothing; one that came back to life sets them first, as
    /// each modifier's policy says (04-capabilities/stats.md, An interval's policy at a respawn).
    pub(crate) fn run_intervals(
        world: &mut World,
        carriers: &mut Carriers,
        revived: &mut SystemState<Removals<'_, '_, Dead>>,
        (mut due, mut back): (Local<'_, Vec<IntervalDue>>, Local<'_, Vec<Entity>>),
    ) {
        let now = world.resource::<SimTick>().start();
        let granting = world.resource::<TagBook>().granting();
        let book = world.resource::<ModifierBook>().clone();
        back.clear();
        let mut revived = revived
            .get_mut(world)
            .expect("the removals of `Dead` are always valid");
        back.extend(revived.read());
        for &entity in &*back {
            if let Ok((_, modifiers, clocks, _, false)) = carriers.get_mut(world, entity) {
                CarriedMut::new(modifiers, clocks).revive_intervals(now, |id| book.revival(id));
            }
        }
        due.clear();
        for (&carrier, modifiers, clocks, tags, dead) in carriers.iter_mut(world) {
            let mut carried = CarriedMut::new(modifiers, clocks);
            if dead {
                carried.skip_intervals(now);
                continue;
            }
            let immune = tags.map_or_else(TagSet::default, |tags| tags.immune);
            let takes_effect = TagEffect::new(granting, immune);
            let push = |id, source| {
                due.push(IntervalDue {
                    carrier,
                    id,
                    source,
                });
            };
            carried.advance_intervals(now, |id| takes_effect.takes(book.tags(id)), push);
        }
        if due.is_empty() {
            return;
        }
        due.sort_unstable();
        let Some(hooks) = ModifierEvents::of(world) else {
            return;
        };
        world.script_batch(|batch| {
            for &IntervalDue {
                carrier,
                id,
                source,
            } in &*due
            {
                let heard = Heard { id, source };
                hooks.call(batch, carrier, heard, Hook::OnInterval, 1, None);
            }
        });
    }
}
