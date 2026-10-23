use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_script::{Answered, CallStart, Hook, ScriptId, ScriptsWorld};
use campfire_sim::{DueIndex, DueUnit, SimTick, StableId};
use campfire_stats::UnitPools;
use campfire_units::{Dead, Destination, Due};
use campfire_values::UnitType;

use crate::ai::{Ai, NextThink, OnThink};
use crate::resetting::Resetting;

/// A unit due to think this tick: since which tick, and with which script and period.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ThinkDue {
    since: Tick,
    id: StableId,
    entity: Entity,
    script: ScriptId,
    period: Ticks,
}

/// The units' AI: the resets of the dead, and the calls of the units due to think.
#[derive(Debug)]
pub struct Thinking;

impl Thinking {
    /// Ends the reset of each unit that died since the last Think stage, with nothing more: its AI
    /// thinks free of it when it lives again. No order reaches a dead unit, so it starts no reset.
    pub(super) fn end_dead_resets(
        mut commands: Commands<'_, '_>,
        units: Query<'_, '_, Entity, (Added<Dead>, With<Resetting>)>,
    ) {
        for entity in &units {
            commands.entity(entity).remove::<Resetting>();
        }
    }

    /// Collects each living unit of a type with AI that is due, those due longest first, then by
    /// stable id, for `think` to run. A unit is first due in the first tick that leaves the
    /// remainder of its stable id when divided by its type's period, so the units of a type spread
    /// over the period; then a period after each think. First, each reset whose unit arrived, its
    /// destination dropped, ends with its pools full.
    pub(crate) fn collect_thinkers(
        (tick, book): (Res<'_, SimTick>, Res<'_, ById<UnitType, Ai>>),
        mut resetting: Query<
            '_,
            '_,
            (Entity, Option<&Destination>, UnitPools),
            (With<Resetting>, Without<Dead>),
        >,
        thinkers: Query<'_, '_, (&UnitType, &NextThink), Without<Dead>>,
        mut due: ResMut<'_, Due<ThinkDue>>,
        mut commands: Commands<'_, '_>,
        (mut index, mut taken): (ResMut<'_, DueIndex<NextThink>>, Local<'_, Vec<DueUnit>>),
    ) {
        let now = tick.start();
        for (entity, destination, mut pools) in &mut resetting {
            if destination.is_none_or(|destination| destination.get().is_none()) {
                commands.entity(entity).remove::<Resetting>();
                pools.fill();
            }
        }
        taken.clear();
        index.take_due(now, &mut taken);
        let thinks = |&DueUnit { id, entity }: &DueUnit| {
            let (&unit_type, &next) = thinkers.get(entity).ok()?;
            let ai = book.get(unit_type)?;
            let since = next.due_at(id, ai.period, now);
            if since > now {
                index.push(since, id, entity);
                return None;
            }
            Some(ThinkDue {
                since,
                id,
                entity,
                script: ai.script,
                period: ai.period,
            })
        };
        due.refill(taken.iter().filter_map(thinks));
        due.as_mut_slice()
            .sort_unstable_by_key(|due| (due.since, due.id));
    }

    /// Runs `on_think` for each unit `collect_thinkers` found due, in its order. Each call's
    /// orders apply when it returns; a failed call's do not. A unit whose call finds the think
    /// pool spent stays due, so under load AI thinks later, and no unit misses its turn for good.
    pub(crate) fn think(world: &mut World) {
        let due = world.resource_mut::<Due<ThinkDue>>().take();
        if !due.is_empty() {
            Self::run(world, &due);
        }
        world.resource_mut::<Due<ThinkDue>>().give_back(due);
    }

    /// Runs the calls of `due`.
    fn run(world: &mut World, due: &[ThinkDue]) {
        let now = world.resource::<SimTick>().start();
        world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            for &ThinkDue {
                since,
                id,
                entity,
                script,
                period,
            } in due
            {
                // A unit with no position or team is no unit scripts see: it stays due, as one
                // whose pool is spent, and thinks once the view holds it.
                let Some(unit) = ctx.view().unit(id) else {
                    batch
                        .world()
                        .entity_mut(entity)
                        .insert(NextThink::new(since));
                    continue;
                };
                let args = (ctx.clone(), unit);
                let hook = Hook::of::<OnThink>();
                let thought = batch.answer(CallStart::think(id, now), hook, Some(id), |batch| {
                    batch.call::<OnThink>(script, args)
                });
                // A unit whose think finds the pool spent thinks again in a later tick.
                let next = match thought {
                    Answered::Ran(_) | Answered::Failed => now.after(period),
                    Answered::Waits => since,
                };
                batch
                    .world()
                    .entity_mut(entity)
                    .insert(NextThink::new(next));
            }
        });
    }
}
