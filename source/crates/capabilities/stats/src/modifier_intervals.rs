use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_script::{Ctx, HookDecl, ScriptRole, ScriptsWorld};
use campfire_sim::{DueUnit, Removals, SimTick, StableId};
use campfire_units::{Dead, Due, TagBook, TagEffect};
use campfire_values::{ModifierId, TagSet, UnitTags};

use crate::carried_mut::CarriedMut;
use crate::modifier_book::ModifierBook;
use crate::modifier_dues::ModifierIndexes;
use crate::modifier_events::ModifierEvents;
use crate::modifier_handle::ModifierHandle;
use crate::modifier_hooks::{Heard, ModifierHook};
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
type Carriers<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static mut Modifiers,
        &'static mut ModifierClocks,
        Option<&'static UnitTags>,
        Has<Dead>,
    ),
>;

/// `on_interval(ctx, m)`: a modifier's interval passes.
#[derive(Debug)]
pub(crate) struct OnInterval;

impl HookDecl for OnInterval {
    const NAME: &'static str = "on_interval";
    const ROLE: ScriptRole = ScriptRole::Modifier;
    const PARAMS: &'static [&'static str] = &["ctx", "m"];
    type Args = (Ctx, ModifierHandle);
}

impl ModifierHook for OnInterval {
    type Event = ();

    fn args(ctx: Ctx, m: ModifierHandle, (): ()) -> (Ctx, ModifierHandle) {
        (ctx, m)
    }
}

impl ModifierIntervals {
    /// Counts the intervals of each carrier the index of the intervals holds due, and collects
    /// each instance whose interval comes this tick, by carrier's stable id, then modifier, then
    /// source, for `run_intervals` in a match that runs scripts. A dead carrier's intervals keep
    /// the clock and run nothing; one that came back to life sets them first, as each modifier's
    /// policy says (04-capabilities/stats.md, An interval's policy at a respawn).
    pub(crate) fn count_intervals(
        (tick, tags, book): (Res<'_, SimTick>, Res<'_, TagBook>, Res<'_, ModifierBook>),
        mut carriers: Carriers<'_, '_>,
        mut revived: Removals<'_, '_, Dead>,
        (mut indexes, mut carrying): (ModifierIndexes<'_>, Local<'_, Vec<DueUnit>>),
        mut due: Option<ResMut<'_, Due<IntervalDue>>>,
    ) {
        let now = tick.start();
        let granting = tags.granting();
        for entity in revived.read() {
            if let Ok((&carrier, modifiers, clocks, _, false)) = carriers.get_mut(entity) {
                let dues = indexes.of(carrier, entity);
                let mut carried = CarriedMut::new(modifiers, clocks, dues);
                carried.revive_intervals(now, |id| book.revival(id));
            }
        }
        carrying.clear();
        indexes.take_intervals(now, &mut carrying);
        for &DueUnit { entity, .. } in &*carrying {
            let Ok((&carrier, modifiers, clocks, tags, dead)) = carriers.get_mut(entity) else {
                continue;
            };
            let mut carried = CarriedMut::new(modifiers, clocks, indexes.of(carrier, entity));
            if dead {
                carried.skip_intervals(now);
                continue;
            }
            let immune = tags.map_or_else(TagSet::default, |tags| tags.immune);
            let takes_effect = TagEffect::new(granting, immune);
            let push = |id, source| {
                if let Some(due) = due.as_deref_mut() {
                    due.push(IntervalDue {
                        carrier,
                        id,
                        source,
                    });
                }
            };
            carried.advance_intervals(now, |id| takes_effect.takes(book.tags(id)), push);
        }
        if let Some(due) = due.as_deref_mut() {
            due.as_mut_slice().sort_unstable();
        }
    }

    /// Runs `on_interval` of each instance `count_intervals` collected, in its order.
    pub(crate) fn run_intervals(world: &mut World) {
        let due = world.resource_mut::<Due<IntervalDue>>().take();
        if let Some(hooks) = ModifierEvents::of(world).filter(|_| !due.is_empty()) {
            world.script_batch(|batch| {
                for &IntervalDue {
                    carrier,
                    id,
                    source,
                } in &due
                {
                    let heard = Heard { id, source };
                    hooks.call::<OnInterval>(batch, carrier, heard, 1, ());
                }
            });
        }
        world.resource_mut::<Due<IntervalDue>>().give_back(due);
    }
}
