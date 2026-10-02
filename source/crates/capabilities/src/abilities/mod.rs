use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_script::ScriptId;
use campfire_script::rhai::Dynamic;
use campfire_sim::{
    Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry, Tick, TickRate, Ticks,
};

use crate::abilities::effect_lists::EffectLists;

use crate::actions::action_book::{ActionBook, ActionId, Delivery};

use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::{ActionSlots, ActionTarget};
use crate::actions::purse::Purse;
use crate::areas::Areas;
use crate::combat::CombatSet;

use crate::combat::dead::Dead;
use crate::deliveries::delivering::Delivering;
use crate::mode::player_resources::PlayerResources;

use crate::projectiles::Projectiles;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;

use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;

use crate::stats::pool_cost::PoolCost;

use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::owner::Owner;

use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;

pub(crate) mod abilities_api;
pub(crate) mod effect_lists;
pub(crate) mod effect_names;

/// The `abilities` capability: abilities in slots, cast through their checks, with the effect a
/// script describes.
#[derive(Debug)]
pub struct Abilities;

impl Abilities {
    /// Adds abilities to a match, on the core `Units` installs: in Hit, after attacks strike and
    /// before the tick's projectiles launch, due casts resolve: the delivery, the cost, the
    /// cooldown and the script's effects apply together, or none of them. A cast resolves in the
    /// script host, so without the core's scripts, as on a client, which predicts no casts, it
    /// installs nothing.
    pub fn install(world: &mut World, schedule: &mut Schedule, _: &mut StateRegistry) {
        if !world.contains_non_send::<Ctx>() {
            return;
        }
        world.insert_resource(EffectLists::default());
        schedule.add_systems(
            resolve_casts
                .in_set(SimSet::Hit)
                .after(CombatSet::Strike)
                .before(CombatSet::Launch),
        );
    }
}

/// Resolves the casts due this tick, in the order of their caster's stable id. Their calls share
/// one snapshot of the living units: effects apply only in Resolve, so none changes it. A due cast
/// whose caster's tags keep it from casting goes back to its order instead.
fn resolve_casts(
    world: &mut World,
    casters: &mut QueryState<(Entity, &StableId, &ActionSlots), Without<Dead>>,
    (mut order, mut due): (Local<'_, Ordered>, Local<'_, Vec<Keyed>>),
) {
    let now = world.resource::<SimTick>().start();
    let resolving = casters
        .iter(world)
        .filter(|(.., slots)| {
            slots
                .in_progress()
                .filter(|underway| underway.kind == ActionKind::Cast)
                .and_then(|casting| casting.resolves_at)
                .is_some_and(|at| at <= now)
        })
        .map(|(entity, &id, _)| Keyed { id, entity });
    due.clear();
    due.extend_from_slice(order.sort(resolving));
    due.retain(|&Keyed { entity, .. }| {
        let can_cast = !UnitTags::effects_of(world.get::<UnitTags>(entity)).blocks(Block::Cast);
        if !can_cast {
            world
                .get_mut::<ActionSlots>(entity)
                .expect("a due caster has slots")
                .interrupt();
        }
        can_cast
    });
    if due.is_empty() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &Keyed { id: caster, entity } in &*due {
            resolve(batch, &ctx, now, caster, entity);
        }
    });
}

/// A cast ready to run: the caster as the script sees it, the pool its call draws from, its
/// slot, action and rank, its target as it aimed and as the script sees it, its `on_resolve`,
/// and its cost and cooldown. Its params wait in the frame.
#[derive(Debug)]
struct Prepared {
    caster: Unit,
    pool: Pool,
    slot: u8,
    action: ActionId,
    rank: u8,
    aim: ActionTarget,
    target: Dynamic,
    on_resolve: Option<ScriptId>,
    cost: PoolCost,
    cooldown: Ticks,
}

/// Resolves one cast: its script runs, then its effects, cost and cooldown apply together, or,
/// when the cast no longer passes its checks or it fails, none of them.
fn resolve(batch: &mut ScriptBatch<'_>, ctx: &Ctx, now: Tick, caster: StableId, entity: Entity) {
    let prepared = prepare(batch.world(), ctx, now, caster, entity);
    let outcome = match prepared {
        Ok(None) => Ok(()),
        Ok(Some(mut prepared)) => run(batch, ctx, &mut prepared).map(|()| {
            apply(batch.world(), ctx, now, entity, &prepared);
        }),
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        batch.record(Some(caster), Hook::OnResolve, error);
    }
    batch
        .world()
        .get_mut::<ActionSlots>(entity)
        .expect("a caster has slots")
        .stop();
}

/// Applies a cast that ran: its delivery's launches, then the effects it queued in `frame` and
/// its handle writes, its cost and its cooldown.
fn apply(world: &mut World, ctx: &Ctx, now: Tick, entity: Entity, prepared: &Prepared) {
    let from = *world.get::<Position>(entity).expect("a caster stands");
    let by = Delivering {
        source: prepared.caster.id,
        action: prepared.action,
        rank: prepared.rank,
    };
    let book = world.resource::<ActionBook>();
    match book.get(by.action).and_then(|action| action.delivery) {
        Some(Delivery::Projectile(fan)) => {
            Projectiles::deliver(world, by, from, fan, prepared.aim);
        }
        Some(Delivery::Area) => Areas::deliver(world, by, from, prepared.aim),
        None => {}
    }
    ctx.apply(world, now);
    if let Some(mut pools) = world.get_mut::<Pools>(entity) {
        pools.pay(&prepared.cost);
    }
    world
        .get_mut::<ActionSlots>(entity)
        .expect("a caster has slots")
        .cool_down(prepared.slot, now.after(prepared.cooldown));
}

/// The cast of `entity` checked again, and its params at its rank put in the frame; `None` when
/// it no longer passes its checks, or its caster is no unit the view read.
fn prepare(
    world: &World,
    ctx: &Ctx,
    now: Tick,
    caster: StableId,
    entity: Entity,
) -> Result<Option<Prepared>, CallError> {
    let view = ctx.view();
    let Some(caster) = view.unit(caster) else {
        return Ok(None);
    };
    let unit = world.entity(entity);
    let slots = unit.get::<ActionSlots>().expect("a due caster has slots");
    let casting = slots.in_progress().expect("a due caster casts");
    let team = *unit.get::<Team>().expect("a caster has a team");
    let book = world.resource::<ActionBook>();
    let owner = unit.get::<Owner>().map(|owner| owner.slot());
    let purse = Purse {
        pools: unit.get::<Pools>(),
        resources: world.get_resource::<PlayerResources>(),
        owner,
    };
    let living = |id| view.living(id);
    let attitude = |other| view.attitude(team, other);
    let Some(checked) = book.check(now, slots, purse, casting, attitude, living) else {
        return Ok(None);
    };
    let target = match casting.target {
        ActionTarget::None => Dynamic::UNIT,
        ActionTarget::Unit(id) => view
            .living(id)
            .and_then(|_| view.unit(id))
            .map_or(Dynamic::UNIT, Dynamic::from),
        ActionTarget::Point(at) => Dynamic::from(at),
    };
    let mut frame = ctx.frame();
    let package = checked.action.package;
    frame.begin_cast(world, checked.id, checked.rank, caster.id, package, None)?;
    let resource_cost = checked.action.resource_cost(checked.rank);
    if let (Some(owner), false) = (owner, resource_cost.is_empty()) {
        frame
            .resources_mut()
            .expect("a purse that affords player resources is a match's")
            .pay(owner, resource_cost);
    }
    drop(frame);
    let pool = owner.map_or(Pool::Think, Pool::Player);
    Ok(Some(Prepared {
        caster,
        pool,
        slot: casting.slot,
        action: checked.id,
        rank: checked.rank,
        aim: checked.target,
        target,
        on_resolve: checked.action.hook(Hook::OnResolve),
        cost: checked.values.cost,
        cooldown: checked.values.cooldown,
    }))
}

/// Queues the prepared cast's `on_resolve` list in the frame, to the unit it aimed at, then runs
/// its script's `on_resolve`, which queues its own effects after it.
fn run(batch: &mut ScriptBatch<'_>, ctx: &Ctx, prepared: &mut Prepared) -> Result<(), CallError> {
    let world = batch.world();
    let list = world
        .resource::<EffectLists>()
        .of(prepared.action, Hook::OnResolve);
    let rate = *world.resource::<TickRate>();
    EffectLists::queue(list, &mut ctx.frame(), prepared.aim.unit(), rate);
    let Some(script) = prepared.on_resolve else {
        return Ok(());
    };
    let target = mem::take(&mut prepared.target);
    let caster = prepared.caster.clone();
    let args = (ctx.clone(), caster, target);
    batch
        .call(prepared.pool, script, Hook::OnResolve, args)
        .map(drop)
        .map_err(CallError::from_script)
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::abilities::Abilities;
    use crate::abilities::effect_lists::EffectLists;
    use crate::abilities::effect_lists::Listed;
    use crate::abilities::effect_names::EffectNames;
    use crate::actions::action_book::ActionId;
    use crate::actions::action_data::ActionData;
    use crate::combat::damage_kind::DamageKind;
    use crate::progression::track_id::TrackId;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::frame::Frame;
    use crate::stats::Stats;
    use crate::stats::modifier_book::ModifierId;
    use crate::stats::pool_id::PoolId;
    use crate::units::script_view::View;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;

    impl Abilities {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which the
        /// package load checked: each name resolved to its id, each param to its place among the
        /// action's params.
        pub fn load_effects(world: &mut World, action: ActionId, package: u16, data: &ActionData) {
            let ctx = world.non_send::<Ctx>().clone();
            let lists = {
                let frame = ctx.frame();
                let names = MatchEffectNames {
                    world,
                    view: ctx.view(),
                    frame: &frame,
                    action,
                    package,
                };
                Listed::lists_of(data, &names)
            };
            world.resource_mut::<EffectLists>().push(action, lists);
        }
    }

    /// The names of an action's effect lists as a match's world resolves them: its view, the frame
    /// that holds the action's params, and the modifiers of the action's package.
    #[derive(Debug)]
    struct MatchEffectNames<'w> {
        world: &'w World,
        view: &'w View,
        frame: &'w Frame,
        action: ActionId,
        package: u16,
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> usize {
            self.frame
                .find_param(self.action, name.as_str())
                .expect("the load checked an effect's param")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind(name.as_str())
                .expect("the load checked an effect's damage kind")
        }

        fn pool(&self, name: &DeclaredName) -> PoolId {
            self.view
                .pool_id(name.as_str())
                .expect("the load checked an effect's pool")
        }

        fn modifier(&self, name: &DeclaredName) -> ModifierId {
            Stats::modifier(self.world, self.package, name.as_str())
                .expect("the load checked an effect's modifier")
        }

        fn track(&self, name: &DeclaredName) -> TrackId {
            self.view
                .track(name.as_str())
                .expect("the load checked an effect's track")
        }
    }
}

#[cfg(test)]
mod tests;
