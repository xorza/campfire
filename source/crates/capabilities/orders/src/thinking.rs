use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, With, Without};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_declare::ByType;
use campfire_script::{CallError, CallStart, Hook, HookCall, Pool, ScriptId, ScriptsWorld};
use campfire_sim::{SimTick, StableId};
use campfire_stats::UnitPools;
use campfire_units::{Dead, Destination, Due};
use campfire_values::UnitType;

use crate::ai::Ai;
use crate::next_think::NextThink;
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
        (tick, book): (Res<'_, SimTick>, Res<'_, ByType<Ai>>),
        mut resetting: Query<
            '_,
            '_,
            (Entity, Option<&Destination>, UnitPools),
            (With<Resetting>, Without<Dead>),
        >,
        thinkers: Query<'_, '_, (Entity, &StableId, &UnitType, &NextThink), Without<Dead>>,
        mut due: ResMut<'_, Due<ThinkDue>>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = tick.start();
        for (entity, destination, mut pools) in &mut resetting {
            if destination.is_none_or(|destination| destination.get().is_none()) {
                commands.entity(entity).remove::<Resetting>();
                pools.fill();
            }
        }
        due.refill(
            thinkers
                .iter()
                .filter_map(|(entity, &id, &unit_type, next)| {
                    let ai = book.get(unit_type)?;
                    let period = ai.period;
                    let since = match next.get() {
                        Some(next) => next,
                        None if now.get() % period.get() == id.get() % period.get() => now,
                        None => return None,
                    };
                    (since <= now).then_some(ThinkDue {
                        since,
                        id,
                        entity,
                        script: ai.script,
                        period,
                    })
                }),
        );
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
                // A unit with no position or team is no unit scripts see.
                let Some(unit) = ctx.view().unit(id) else {
                    continue;
                };
                let args = (ctx.clone(), unit);
                let thought = batch.run(CallStart::think(id, now), |batch| {
                    let call = HookCall {
                        script,
                        hook: Hook::OnThink,
                        args,
                    };
                    batch.call(Pool::Think, call)
                });
                // A unit whose think finds the pool spent thinks again in a later tick.
                let next = match thought {
                    Ok(_) => now.after(period),
                    Err(CallError::PoolSpent) => since,
                    Err(error) => {
                        batch.record(Some(id), Hook::OnThink, &error);
                        now.after(period)
                    }
                };
                *batch
                    .world()
                    .get_mut::<NextThink>(entity)
                    .expect("a unit that thinks holds its next think") = NextThink::new(next);
            }
        });
    }
}
