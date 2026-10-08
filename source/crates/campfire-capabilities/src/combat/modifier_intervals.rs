use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_sim::{SimTick, StableId};

use crate::combat::combat_event::{CombatEvent, CombatEvents};
use crate::combat::interval_due::IntervalDue;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::Modifiers;
use crate::units::dead::Dead;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::unit_tags::UnitTags;
use crate::units::view::View;

/// The intervals of the modifiers units carry, which run their hooks in Hit.
#[derive(Debug)]
pub(super) struct ModifierIntervals;

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
        let granting = world
            .get_resource::<TagBook>()
            .map_or(TagSet::default(), TagBook::granting);
        let book = world.resource::<ModifierBook>().clone();
        due.clear();
        for (&carrier, modifiers, clocks, tags) in carriers.iter_mut(world) {
            let immune = tags.map_or(TagSet::default(), |tags| tags.immune);
            let takes_effect = TagBook::effect_test(granting, immune);
            let push = |id, source| {
                due.push(IntervalDue {
                    carrier,
                    id,
                    source,
                });
            };
            CarriedMut::new(modifiers, clocks).advance_intervals(
                now,
                |id| takes_effect(book.tags(id)),
                push,
            );
        }
        if due.is_empty() {
            return;
        }
        due.sort_unstable();
        let Some(events) = world.remove_non_send::<CombatEvents>() else {
            return;
        };
        let view = world.non_send::<View>().clone();
        ScriptBatch::run(world, &view, |batch| {
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
        world.insert_non_send(events);
    }
}
