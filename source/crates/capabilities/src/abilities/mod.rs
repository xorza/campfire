use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_script::rhai::Dynamic;
use campfire_script::{Budget, ScriptHost, ScriptId};
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::abilities::ability_book::{Ability, AbilityBook, AbilityId};
use crate::abilities::ability_data::{AbilityData, Range, Targeting};
use crate::abilities::ability_slots::{AbilitySlots, CastTarget, Casting};
use crate::abilities::error::AbilityError;
use crate::abilities::frame::{Effect, Frame};
use crate::abilities::resource_pool::ResourcePool;
use crate::abilities::script_api::Ctx;
use crate::combat::CombatSet;
use crate::combat::attack_stats::ground_offset;
use crate::combat::dead::Dead;
use crate::combat::living_unit::LivingUnit;
use crate::combat::strikes::{Strike, Strikes};
use crate::combat::targets::Targets;
use crate::combat::team::Team;
use crate::units::error::CallError;
use crate::units::hook::Hook;
use crate::units::param::Param;
use crate::units::scalar::Scalar;
use crate::units::script_budgets::ScriptBudgets;
use crate::units::script_failures::{ScriptFailure, ScriptFailures};
use crate::units::script_view::View;
use crate::units::unit::Unit;

pub(crate) mod ability_book;
pub(crate) mod ability_data;
pub(crate) mod ability_slots;
pub(crate) mod error;
pub(crate) mod frame;
pub(crate) mod resource_pool;
pub(crate) mod script_api;

/// The `abilities` capability: abilities in slots, cast through their checks, with the effect a
/// script describes.
#[derive(Debug)]
pub struct Abilities;

impl Abilities {
    /// Adds abilities to a match, on the core `Units` installs. In Act, ordered casts pass their
    /// checks and start; in Hit, after attacks strike and fire, due casts resolve: the cost, the
    /// cooldown and the script's effects apply together, or none of them.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        Ctx::register(
            world
                .get_non_send_mut::<ScriptHost>()
                .expect("Units::install runs first")
                .into_inner()
                .engine_mut(),
        );
        let ctx = Ctx::new(world.non_send::<View>().clone());
        world.insert_non_send(ctx);
        world.insert_resource(AbilityBook::default());
        schedule.add_systems((
            start_casts.in_set(SimSet::Act),
            resolve_casts.in_set(SimSet::Hit).after(CombatSet::Launch),
        ));
        registry.register_component::<AbilitySlots>();
        registry.register_component::<ResourcePool>();
    }

    /// Loads an ability into the match, with the source of its script if its data names one:
    /// times in milliseconds become ticks at the match's rate, rounded up.
    pub fn load(
        world: &mut World,
        data: &AbilityData,
        source: Option<&str>,
    ) -> Result<AbilityId, AbilityError> {
        let rate = *world.resource::<TickRate>();
        let mut host = world
            .remove_non_send::<ScriptHost>()
            .expect("units are installed");
        let ctx = world.non_send::<Ctx>().clone();
        let loaded = world.resource_mut::<AbilityBook>().load(
            &mut host,
            &mut ctx.frame(),
            data,
            source,
            rate,
        );
        world.insert_non_send(host);
        loaded
    }
}

/// Starts each ordered cast that passes its checks, its target within range, and drops the
/// others.
fn start_casts(
    tick: Res<'_, SimTick>,
    book: Res<'_, AbilityBook>,
    targets: Targets<'_, '_>,
    mut casters: Query<
        '_,
        '_,
        (&Position, &Team, &mut AbilitySlots, Option<&ResourcePool>),
        Without<Dead>,
    >,
) {
    let now = tick.get();
    for (&position, &team, mut slots, pool) in &mut casters {
        let Some(casting) = slots
            .casting()
            .filter(|casting| casting.resolves_at.is_none())
        else {
            continue;
        };
        let lookup = |id| targets.living(id);
        let started = check(&book, now, &slots, pool, team, casting, lookup)
            .filter(|checked| in_range(checked, position, casting.target, lookup))
            .map(|checked| now + checked.cast_time);
        match started {
            Some(resolves_at) => slots.start(resolves_at),
            None => slots.stop(),
        }
    }
}

/// A cast that passes its checks: its ability, and the values at the slot's rank.
#[derive(Debug)]
struct Checked<'a> {
    id: AbilityId,
    ability: &'a Ability,
    rank: u8,
    range: Range,
    cost: Num,
    cooldown: u64,
    cast_time: u64,
}

/// The cast `casting` of a unit on `team`, when it may go on: its slot holds a learned ability
/// that is ready, its cost is affordable, and its target is a living unit of the relation the
/// ability takes. `living` finds a living unit.
fn check<'a>(
    book: &'a AbilityBook,
    now: u64,
    slots: &AbilitySlots,
    pool: Option<&ResourcePool>,
    team: Team,
    casting: Casting,
    living: impl Fn(StableId) -> Option<LivingUnit>,
) -> Option<Checked<'a>> {
    let slot = slots.slot(casting.slot).filter(|slot| slot.rank > 0)?;
    let ability = book.get(slot.ability)?;
    let cost = Num::from_int(i64::try_from(ability.cost.at(slot.rank)?).ok()?)?;
    if now < slot.ready_at || cost > pool.map_or(Num::ZERO, |pool| pool.current()) {
        return None;
    }
    let target_fits = match (ability.targeting, casting.target) {
        (Targeting::None, CastTarget::None) => true,
        (Targeting::Unit(relation), CastTarget::Unit(target)) => {
            living(target).is_some_and(|unit| relation.holds(team, unit.team))
        }
        _ => false,
    };
    target_fits.then_some(Checked {
        id: slot.ability,
        ability,
        rank: slot.rank,
        range: ability.range.at(slot.rank)?,
        cost,
        cooldown: ability.cooldown.at(slot.rank)?,
        cast_time: ability.cast_time.at(slot.rank)?,
    })
}

/// Whether a unit target is within the ability's range of `position` on the ground plane. The
/// range counts only when a cast starts.
fn in_range(
    checked: &Checked<'_>,
    position: Position,
    target: CastTarget,
    living: impl Fn(StableId) -> Option<LivingUnit>,
) -> bool {
    let (Range::Meters(range), CastTarget::Unit(target)) = (checked.range, target) else {
        return true;
    };
    living(target).is_some_and(|unit| Vec3::ZERO.within(ground_offset(position, unit.pos), range))
}

/// Resolves the casts due this tick, in the order of their caster's stable id. Their calls share
/// one snapshot of the living units: effects apply only in Resolve, so none changes it.
fn resolve_casts(world: &mut World, mut due: Local<'_, Vec<(StableId, Entity)>>) {
    let now = world.resource::<SimTick>().get();
    due.clear();
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        let caster = world.entity(entity);
        let resolves = caster
            .get::<AbilitySlots>()
            .and_then(AbilitySlots::casting)
            .and_then(|casting| casting.resolves_at)
            .is_some_and(|at| at <= now);
        if resolves && !caster.contains::<Dead>() {
            due.push((id, entity));
        }
    }
    if due.is_empty() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    ctx.view().read(world);
    let mut host = world
        .remove_non_send::<ScriptHost>()
        .expect("units are installed");
    let mut budget = world.resource::<ScriptBudgets>().input;
    for &(caster, entity) in &*due {
        resolve(world, &mut host, &mut budget, &ctx, now, caster, entity);
    }
    world.resource_mut::<ScriptBudgets>().input = budget;
    world.insert_non_send(host);
}

/// A cast ready to run: the caster as the script sees it, its target, its `on_cast`, and its
/// cost and cooldown. Its params wait in the frame.
#[derive(Debug)]
struct Prepared {
    caster: Unit,
    slot: u8,
    target: Dynamic,
    on_cast: Option<ScriptId>,
    cost: Num,
    cooldown: u64,
}

/// Resolves one cast: its script runs, then its effects, cost and cooldown apply together, or,
/// when the cast no longer passes its checks or it fails, none of them.
fn resolve(
    world: &mut World,
    host: &mut ScriptHost,
    budget: &mut Budget,
    ctx: &Ctx,
    now: u64,
    caster: StableId,
    entity: Entity,
) {
    let prepared = prepare(world, ctx, now, caster, entity);
    let outcome = match prepared {
        Ok(None) => Ok(()),
        Ok(Some(mut prepared)) => run(host, budget, ctx, &mut prepared).map(|()| {
            apply(world, &mut ctx.frame(), now, entity, &prepared);
        }),
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        world
            .non_send_mut::<ScriptFailures>()
            .0
            .push(ScriptFailure {
                unit: Some(caster),
                hook: Hook::OnCast,
                error,
            });
    }
    world
        .get_mut::<AbilitySlots>(entity)
        .expect("a caster has slots")
        .stop();
}

/// Applies a cast that ran: the effects it queued in `frame`, its cost and its cooldown.
fn apply(world: &mut World, frame: &mut Frame, now: u64, entity: Entity, prepared: &Prepared) {
    for effect in frame.effects.drain(..) {
        match effect {
            Effect::Damage { target, amount } => {
                world.resource_mut::<Strikes>().0.push(Strike {
                    source: prepared.caster.id,
                    target,
                    amount,
                });
            }
        }
    }
    if let Some(mut pool) = world.get_mut::<ResourcePool>(entity) {
        pool.spend(prepared.cost);
    }
    world
        .get_mut::<AbilitySlots>(entity)
        .expect("a caster has slots")
        .cool_down(prepared.slot, now + prepared.cooldown);
}

/// The cast of `entity` checked again, and its params at its rank put in the frame; `None` when
/// it no longer passes its checks, or its caster is no unit the view read.
fn prepare(
    world: &World,
    ctx: &Ctx,
    now: u64,
    caster: StableId,
    entity: Entity,
) -> Result<Option<Prepared>, CallError> {
    let view = ctx.view();
    let Some(caster) = view.unit(caster) else {
        return Ok(None);
    };
    let unit = world.entity(entity);
    let slots = unit.get::<AbilitySlots>().expect("a due caster has slots");
    let casting = slots.casting().expect("a due caster casts");
    let team = *unit.get::<Team>().expect("a caster has a team");
    let book = world.resource::<AbilityBook>();
    let pool = unit.get::<ResourcePool>();
    let Some(checked) = check(book, now, slots, pool, team, casting, |id| view.living(id)) else {
        return Ok(None);
    };
    let target = match casting.target {
        CastTarget::None => Dynamic::UNIT,
        CastTarget::Unit(id) => view
            .living(id)
            .and_then(|_| view.unit(id))
            .map_or(Dynamic::UNIT, Dynamic::from),
    };
    let rank = checked.rank;
    let params = checked.ability.params.iter();
    ctx.frame()
        .begin_cast(checked.id, params.map(|param| param_value(param, rank)))?;
    Ok(Some(Prepared {
        caster,
        slot: casting.slot,
        target,
        on_cast: checked.ability.on_cast,
        cost: checked.cost,
        cooldown: checked.cooldown,
    }))
}

/// Runs the prepared cast's `on_cast`, which queues its effects in the frame.
fn run(
    host: &mut ScriptHost,
    budget: &mut Budget,
    ctx: &Ctx,
    prepared: &mut Prepared,
) -> Result<(), CallError> {
    let Some(script) = prepared.on_cast else {
        return Ok(());
    };
    let target = mem::take(&mut prepared.target);
    let caster = prepared.caster.clone();
    host.call(
        budget,
        script,
        Hook::OnCast.name(),
        (ctx.clone(), caster, target),
    )
    .map(drop)
    .map_err(CallError::from_script)
}

/// A param's value at `rank`. Until levels and stats exist, every unit is level 1 and every
/// stat a scaling table names is 0.
fn param_value(param: &Param, rank: u8) -> Option<Scalar> {
    match param {
        Param::Value(value) => Some(*value),
        Param::PerRank(values) => values.get(usize::from(rank.checked_sub(1)?)).copied(),
        Param::Scaling(scaling) => {
            let base = scaling.base.at(rank)?.to_num()?;
            let per_level = scaling.per_level.map_or(Some(Num::ZERO), Scalar::to_num)?;
            Some(Scalar::Decimal(base.checked_add(per_level)?))
        }
    }
}

#[cfg(test)]
mod tests;
