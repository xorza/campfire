use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_script::ScriptsWorld;
use campfire_sim::{SimTick, StableId};
use campfire_stats::{CarriedMut, ModifierBook, ModifierClocks, Modifiers};
use campfire_units::{Dead, TagBook, TagEffect};
use campfire_values::{TagSet, UnitTags};

use crate::combat_event::CombatEvent;
use crate::combat_hooks::CombatHooks;
use crate::interval_due::IntervalDue;

/// The intervals of the modifiers units carry, which run their hooks in Hit.
#[derive(Debug)]
pub struct ModifierIntervals;

impl ModifierIntervals {
    /// Counts each living carrier's intervals, and runs `on_interval` of each instance whose
    /// interval comes this tick, by carrier's stable id, then modifier, then source.
    pub(super) fn run_intervals(
        world: &mut World,
        carriers: &mut QueryState<
            (
                &StableId,
                &mut Modifiers,
                &mut ModifierClocks,
                Option<&UnitTags>,
            ),
            Without<Dead>,
        >,
        mut due: Local<'_, Vec<IntervalDue>>,
    ) {
        let now = world.resource::<SimTick>().start();
        let granting = world.resource::<TagBook>().granting();
        let book = world.resource::<ModifierBook>().clone();
        due.clear();
        for (&carrier, modifiers, clocks, tags) in carriers.iter_mut(world) {
            let immune = tags.map_or_else(TagSet::default, |tags| tags.immune);
            let takes_effect = TagEffect::new(granting, immune);
            let push = |id, source| {
                due.push(IntervalDue {
                    carrier,
                    id,
                    source,
                });
            };
            CarriedMut::new(modifiers, clocks).advance_intervals(
                now,
                |id| takes_effect.takes(book.tags(id)),
                push,
            );
        }
        if due.is_empty() {
            return;
        }
        due.sort_unstable();
        let Some(events) = world.non_send::<CombatHooks>().events.clone() else {
            return;
        };
        world.script_batch(|batch| {
            for &IntervalDue {
                carrier,
                id,
                source,
            } in &*due
            {
                events.call(
                    batch,
                    CombatEvent::Interval {
                        carrier,
                        id,
                        source,
                    },
                );
            }
        });
    }
}
