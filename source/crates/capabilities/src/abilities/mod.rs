use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, NonSendMut, Query, Res};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Dynamic, Map};
use campfire_script::{ScriptError, ScriptHost, ScriptId, ScriptLimits};
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry};

use crate::abilities::ability_book::{Ability, AbilityBook, AbilityId};
use crate::abilities::ability_data::{AbilityData, Param, Range, Relation, Scalar, Targeting};
use crate::abilities::ability_slots::{AbilitySlots, CastTarget, Casting};
use crate::abilities::cast_failures::{CastFailure, CastFailures};
use crate::abilities::error::{AbilityError, ApiError, CastError};
use crate::abilities::resource_pool::ResourcePool;
use crate::abilities::script_api::{Ctx, Effect, Frame, UnitHandle};
use crate::combat::CombatSet;
use crate::combat::attack_stats::ground_offset;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::strikes::{Strike, Strikes};
use crate::combat::targets::Targets;
use crate::combat::team::Team;

pub(crate) mod ability_book;
pub(crate) mod ability_data;
pub(crate) mod ability_slots;
pub(crate) mod cast_failures;
pub(crate) mod error;
pub(crate) mod resource_pool;
pub(crate) mod script_api;

/// The `abilities` capability: abilities in slots, cast through their checks, with the effect a
/// script describes.
#[derive(Debug)]
pub struct Abilities;

impl Abilities {
    /// Adds abilities to a match, scripts running within `limits`. In Inputs, the tick's script
    /// budget starts; in Act, ordered casts pass their checks and start; in Hit, after attacks
    /// strike, due casts resolve: the cost, the cooldown and the script's effects apply
    /// together, or none of them.
    pub fn install(
        world: &mut World,
        schedule: &mut Schedule,
        registry: &mut StateRegistry,
        limits: ScriptLimits,
    ) {
        let mut host = ScriptHost::new(limits);
        Ctx::register(host.engine_mut());
        world.insert_non_send(host);
        world.insert_resource(AbilityBook::default());
        world.insert_non_send(CastFailures::default());
        schedule.add_systems((
            begin_tick.in_set(SimSet::Inputs),
            start_casts.in_set(SimSet::Act),
            resolve_casts.in_set(SimSet::Hit).after(CombatSet::Strike),
        ));
        registry.register_component::<AbilitySlots>();
        registry.register_component::<ResourcePool>();
    }

    /// Loads an ability into the match; see `AbilityBook::load`.
    pub fn load(
        world: &mut World,
        data: &AbilityData,
        source: Option<&str>,
        tick_hz: u32,
    ) -> Result<AbilityId, AbilityError> {
        let mut host = world
            .remove_non_send::<ScriptHost>()
            .expect("abilities are installed");
        let loaded = world
            .resource_mut::<AbilityBook>()
            .load(&mut host, data, source, tick_hz);
        world.insert_non_send(host);
        loaded
    }
}

fn begin_tick(mut host: NonSendMut<'_, ScriptHost>, mut failures: NonSendMut<'_, CastFailures>) {
    host.begin_tick();
    failures.0.clear();
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
    ability: &'a Ability,
    rank: u8,
    range: Range,
    cost: Num,
    cooldown: u64,
    cast_time: u64,
}

/// The cast `casting` of a unit on `team`, when it may go on: its slot holds a learned ability
/// that is ready, its cost is affordable, and its target is a living unit of the relation the
/// ability takes. `living` finds a living unit's position and team.
fn check<'a>(
    book: &'a AbilityBook,
    now: u64,
    slots: &AbilitySlots,
    pool: Option<&ResourcePool>,
    team: Team,
    casting: Casting,
    living: impl Fn(StableId) -> Option<(Position, Team)>,
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
            living(target).is_some_and(|(_, theirs)| match relation {
                Relation::Enemies => team.is_enemy_of(theirs),
                Relation::Allies => !team.is_enemy_of(theirs),
                Relation::All => true,
            })
        }
        _ => false,
    };
    target_fits.then_some(Checked {
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
    living: impl Fn(StableId) -> Option<(Position, Team)>,
) -> bool {
    let (Range::Meters(range), CastTarget::Unit(target)) = (checked.range, target) else {
        return true;
    };
    living(target).is_some_and(|(at, _)| Vec3::ZERO.within(ground_offset(position, at), range))
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
    let units: Rc<[UnitHandle]> = living_units(world).into();
    let mut host = world
        .remove_non_send::<ScriptHost>()
        .expect("abilities are installed");
    for &(caster, entity) in &*due {
        resolve(world, &mut host, now, &units, caster, entity);
    }
    world.insert_non_send(host);
}

/// A cast ready to run: the caster as the script sees it, its target, its script and params, and
/// its cost and cooldown.
#[derive(Debug)]
struct Prepared {
    caster: UnitHandle,
    slot: u8,
    target: Dynamic,
    script: Option<ScriptId>,
    params: Map,
    cost: Num,
    cooldown: u64,
}

/// Resolves one cast: its script runs, then its effects, cost and cooldown apply together, or,
/// when the cast no longer passes its checks or it fails, none of them.
fn resolve(
    world: &mut World,
    host: &mut ScriptHost,
    now: u64,
    units: &Rc<[UnitHandle]>,
    caster: StableId,
    entity: Entity,
) {
    let outcome = match prepare(world, now, units, caster, entity) {
        Ok(None) => Ok(()),
        Ok(Some(mut prepared)) => run(host, units, &mut prepared)
            .map(|effects| apply(world, now, caster, entity, &prepared, effects)),
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        world
            .non_send_mut::<CastFailures>()
            .0
            .push(CastFailure { caster, error });
    }
    world
        .get_mut::<AbilitySlots>(entity)
        .expect("a caster has slots")
        .stop();
}

/// Applies a cast that ran: its effects, cost and cooldown.
fn apply(
    world: &mut World,
    now: u64,
    caster: StableId,
    entity: Entity,
    prepared: &Prepared,
    effects: Vec<Effect>,
) {
    for effect in effects {
        match effect {
            Effect::Damage { target, amount } => {
                world.resource_mut::<Strikes>().0.push(Strike {
                    source: caster,
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

/// The cast of `entity` checked again and its params resolved at its rank; `None` when it no
/// longer passes its checks.
fn prepare(
    world: &World,
    now: u64,
    units: &[UnitHandle],
    caster: StableId,
    entity: Entity,
) -> Result<Option<Prepared>, CastError> {
    let unit = world.entity(entity);
    let slots = unit.get::<AbilitySlots>().expect("a due caster has slots");
    let casting = slots.casting().expect("a due caster casts");
    let team = *unit.get::<Team>().expect("a caster has a team");
    let find = |id| {
        let index = units.binary_search_by_key(&id, |unit: &UnitHandle| unit.id);
        index.ok().map(|index| units[index])
    };
    let lookup = |id| find(id).map(|unit| (unit.pos, unit.team));
    let book = world.resource::<AbilityBook>();
    let pool = unit.get::<ResourcePool>();
    let Some(checked) = check(book, now, slots, pool, team, casting, lookup) else {
        return Ok(None);
    };
    let params = checked
        .ability
        .params
        .iter()
        .map(|(name, param)| Some((name.as_str().into(), param_value(param, checked.rank)?)))
        .collect::<Option<Map>>()
        .ok_or(CastError::ParamOverflow)?;
    let target = match casting.target {
        CastTarget::None => Dynamic::UNIT,
        CastTarget::Unit(id) => find(id).map_or(Dynamic::UNIT, Dynamic::from),
    };
    Ok(Some(Prepared {
        caster: UnitHandle {
            id: caster,
            pos: *unit.get::<Position>().expect("a caster has a position"),
            team,
        },
        slot: casting.slot,
        target,
        script: checked.ability.script,
        params,
        cost: checked.cost,
        cooldown: checked.cooldown,
    }))
}

/// Runs the prepared cast's `on_cast`, and gives the effects it queued.
fn run(
    host: &mut ScriptHost,
    units: &Rc<[UnitHandle]>,
    prepared: &mut Prepared,
) -> Result<Vec<Effect>, CastError> {
    let Some(script) = prepared.script else {
        return Ok(Vec::new());
    };
    let frame = Rc::new(RefCell::new(Frame {
        units: Rc::clone(units),
        params: mem::take(&mut prepared.params),
        effects: Vec::new(),
    }));
    let target = mem::take(&mut prepared.target);
    host.call(
        script,
        "on_cast",
        (Ctx(Rc::clone(&frame)), prepared.caster, target),
    )
    .map(drop)
    .map_err(|error| {
        let refused = match &error {
            ScriptError::Raised(raised) => raised.get::<ApiError>(),
            _ => None,
        };
        refused.map_or(CastError::Script(error), CastError::Api)
    })?;
    let effects = mem::take(&mut frame.borrow_mut().effects);
    Ok(effects)
}

/// The living units with a team and health, by stable id: what a call's queries see.
fn living_units(world: &World) -> Vec<UnitHandle> {
    world
        .resource::<EntityIndex>()
        .iter()
        .filter_map(|(id, entity)| {
            let unit = world.entity(entity);
            if unit.contains::<Dead>() || !unit.contains::<Health>() {
                return None;
            }
            Some(UnitHandle {
                id,
                pos: *unit.get::<Position>()?,
                team: *unit.get::<Team>()?,
            })
        })
        .collect()
}

/// A param's value at `rank`. Until levels and stats exist, every unit is level 1 and every
/// stat a scaling table names is 0.
fn param_value(param: &Param, rank: u8) -> Option<Dynamic> {
    let scalar = |value: Scalar| match value {
        Scalar::Int(value) => Dynamic::from_int(value),
        Scalar::Decimal(value) => Dynamic::from(value),
    };
    match param {
        Param::Value(value) => Some(scalar(*value)),
        Param::PerRank(values) => values
            .get(usize::from(rank.checked_sub(1)?))
            .map(|&v| scalar(v)),
        Param::Scaling(scaling) => {
            let base = scaling.base.at(rank)?.to_num()?;
            let per_level = scaling.per_level.map_or(Some(Num::ZERO), Scalar::to_num)?;
            Some(Dynamic::from(base.checked_add(per_level)?))
        }
    }
}

#[cfg(test)]
mod tests;
