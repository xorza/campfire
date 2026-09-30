use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, NonSend, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_math::Num;
use campfire_script::rhai::Dynamic;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{
    EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, Tick, TickRate, Ticks,
};

use crate::abilities::ability_book::{Ability, AbilityBook, AbilityId, Aim, RankValues};
use crate::abilities::ability_data::{AbilityData, Range, Targeting};
use crate::abilities::ability_slots::{AbilitySlots, CastTarget, Casting};
use crate::abilities::error::AbilityError;
use crate::abilities::frame::{Effect, Frame};
use crate::abilities::resource_pool::ResourcePool;
use crate::abilities::script_api::Ctx;
use crate::combat::CombatSet;
use crate::combat::dead::Dead;
use crate::combat::strikes::{Strike, Strikes};
use crate::combat::targets::Targets;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::units::owner::Owner;
use crate::units::script_view::{RowFill, SlotRow, View};
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;

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
    /// cooldown and the script's effects apply together, or none of them. A cast resolves in the
    /// script host, so without the core's scripts, as on a client, which predicts no casts, it
    /// installs nothing.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let Some(mut host) = world.get_non_send_mut::<ScriptHost>() else {
            return;
        };
        Ctx::register(host.engine_mut());
        let view = world.non_send::<View>().clone();
        view.add_source(fill_row);
        world.insert_non_send(Ctx::new(view));
        world.insert_resource(AbilityBook::default());
        schedule.add_systems((
            start_casts.in_set(SimSet::Act),
            resolve_casts.in_set(SimSet::Hit).after(CombatSet::Launch),
        ));
        registry.register_component::<AbilitySlots>();
        registry.register_component::<ResourcePool>();
    }

    /// Loads an ability of `ranks` ranks into the match, which the package load checked, with
    /// its compiled script exactly when its data names one: its capability fields at each rank,
    /// times in milliseconds as ticks at the match's rate, rounded up.
    pub fn load(
        world: &mut World,
        data: &AbilityData,
        script: Option<ScriptId>,
        ranks: u8,
    ) -> Result<AbilityId, AbilityError> {
        let rate = *world.resource::<TickRate>();
        let aim = match &data.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point,
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(
                world
                    .non_send::<View>()
                    .resolve_filter(filter)
                    .expect("the load checked the filter's tag"),
            ),
        };
        let values = RankValues::all(data, ranks, rate)?;
        let host = world
            .remove_non_send::<ScriptHost>()
            .expect("units are installed");
        let ctx = world.non_send::<Ctx>().clone();
        let id = world.resource_mut::<AbilityBook>().load(
            &host,
            &mut ctx.frame(),
            data,
            script,
            aim,
            values,
        );
        world.insert_non_send(host);
        Ok(id)
    }
}

/// Starts each ordered cast that passes its checks, its target within range, and drops the
/// others.
fn start_casts(
    tick: Res<'_, SimTick>,
    book: Res<'_, AbilityBook>,
    targets: Targets<'_, '_>,
    index: Res<'_, EntityIndex>,
    unit_types: Query<'_, '_, &UnitType>,
    view: NonSend<'_, View>,
    mut casters: Query<
        '_,
        '_,
        (&Position, &Team, &mut AbilitySlots, Option<&ResourcePool>),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, mut slots, pool) in &mut casters {
        let Some(casting) = slots
            .casting()
            .filter(|casting| casting.resolves_at.is_none())
        else {
            continue;
        };
        let lookup = |id| {
            let unit = targets.living(id)?;
            let unit_type = index.get(id).and_then(|entity| unit_types.get(entity).ok());
            Some(TargetUnit {
                pos: unit.pos,
                team: unit.team,
                tags: view.type_tags(unit_type.copied()),
            })
        };
        let started = check(&book, now, &slots, pool, team, casting, lookup)
            .filter(|checked| in_range(checked, position, casting.target, lookup))
            .map(|checked| now.after(checked.cast_time));
        match started {
            Some(resolves_at) => slots.start(resolves_at),
            None => slots.stop(),
        }
    }
}

/// A living unit a cast may target: where it stands, its team and its tags.
#[derive(Debug, Clone, Copy)]
struct TargetUnit {
    pos: Position,
    team: Team,
    tags: TagSet,
}

/// A cast that passes its checks: its ability, and the values at the slot's rank.
#[derive(Debug)]
struct Checked<'a> {
    id: AbilityId,
    ability: &'a Ability,
    rank: u8,
    range: Range,
    cost: Num,
    cooldown: Ticks,
    cast_time: Ticks,
}

/// Fills a unit's ability slots: each one's rank, and how many ranks its ability has.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    let Some(slots) = unit.get::<AbilitySlots>() else {
        return;
    };
    let world = fill.world;
    let book = world.resource::<AbilityBook>();
    let rows = slots.iter().map(|slot| {
        let ability = book
            .get(slot.ability)
            .expect("a slot's ability is in the book");
        SlotRow {
            rank: slot.rank,
            ranks: u8::try_from(ability.ranks.len()).expect("an ability has few ranks"),
        }
    });
    fill.slotted(rows);
}

/// The cast `casting` of a unit on `team`, when it may go on: its slot holds a learned ability
/// that is ready, its cost is affordable, and its target is a living unit the ability's filter
/// selects. `living` finds a living unit.
fn check<'a>(
    book: &'a AbilityBook,
    now: Tick,
    slots: &AbilitySlots,
    pool: Option<&ResourcePool>,
    team: Team,
    casting: Casting,
    living: impl Fn(StableId) -> Option<TargetUnit>,
) -> Option<Checked<'a>> {
    let slot = slots.slot(casting.slot).filter(|slot| slot.rank > 0)?;
    let ability = book.get(slot.ability)?;
    let values = *ability.ranks.get(usize::from(slot.rank - 1))?;
    let cost = Num::from_int(i64::try_from(values.cost).ok()?)?;
    if now < slot.ready_at || cost > pool.map_or(Num::ZERO, |pool| pool.current()) {
        return None;
    }
    let target_fits = match (ability.aim, casting.target) {
        (Aim::None, CastTarget::None) => true,
        (Aim::Unit(filter), CastTarget::Unit(target)) => {
            living(target).is_some_and(|unit| filter.selects(team, unit.team, unit.tags))
        }
        _ => false,
    };
    target_fits.then_some(Checked {
        id: slot.ability,
        ability,
        rank: slot.rank,
        range: values.range,
        cost,
        cooldown: values.cooldown,
        cast_time: values.cast_time,
    })
}

/// Whether a unit target is within the ability's range of `position` on the ground plane. The
/// range counts only when a cast starts.
fn in_range(
    checked: &Checked<'_>,
    position: Position,
    target: CastTarget,
    living: impl Fn(StableId) -> Option<TargetUnit>,
) -> bool {
    let (Range::Meters(range), CastTarget::Unit(target)) = (checked.range, target) else {
        return true;
    };
    living(target).is_some_and(|unit| position.within_ground(unit.pos, range))
}

/// Resolves the casts due this tick, in the order of their caster's stable id. Their calls share
/// one snapshot of the living units: effects apply only in Resolve, so none changes it.
fn resolve_casts(world: &mut World, mut due: Local<'_, Vec<(StableId, Entity)>>) {
    let now = world.resource::<SimTick>().start();
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
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &(caster, entity) in &*due {
            resolve(batch, &ctx, now, caster, entity);
        }
    });
}

/// A cast ready to run: the caster as the script sees it, the pool its call draws from, its
/// target, its `on_cast`, and its cost and cooldown. Its params wait in the frame.
#[derive(Debug)]
struct Prepared {
    caster: Unit,
    pool: Pool,
    slot: u8,
    target: Dynamic,
    on_cast: Option<ScriptId>,
    cost: Num,
    cooldown: Ticks,
}

/// Resolves one cast: its script runs, then its effects, cost and cooldown apply together, or,
/// when the cast no longer passes its checks or it fails, none of them.
fn resolve(batch: &mut ScriptBatch<'_>, ctx: &Ctx, now: Tick, caster: StableId, entity: Entity) {
    let prepared = prepare(batch.world(), ctx, now, caster, entity);
    let outcome = match prepared {
        Ok(None) => Ok(()),
        Ok(Some(mut prepared)) => run(batch, ctx, &mut prepared).map(|()| {
            apply(batch.world(), &mut ctx.frame(), now, entity, &prepared);
        }),
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        batch.record(Some(caster), Hook::OnCast, error);
    }
    batch
        .world()
        .get_mut::<AbilitySlots>(entity)
        .expect("a caster has slots")
        .stop();
}

/// Applies a cast that ran: the effects it queued in `frame`, its cost and its cooldown.
fn apply(world: &mut World, frame: &mut Frame, now: Tick, entity: Entity, prepared: &Prepared) {
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
    let slots = unit.get::<AbilitySlots>().expect("a due caster has slots");
    let casting = slots.casting().expect("a due caster casts");
    let team = *unit.get::<Team>().expect("a caster has a team");
    let book = world.resource::<AbilityBook>();
    let pool = unit.get::<ResourcePool>();
    let living = |id| {
        let unit = view.living(id)?;
        let row = view.row(id)?;
        Some(TargetUnit {
            pos: unit.pos,
            team: unit.team,
            tags: view.type_tags(row.unit_type),
        })
    };
    let Some(checked) = check(book, now, slots, pool, team, casting, living) else {
        return Ok(None);
    };
    let target = match casting.target {
        CastTarget::None => Dynamic::UNIT,
        CastTarget::Unit(id) => view
            .living(id)
            .and_then(|_| view.unit(id))
            .map_or(Dynamic::UNIT, Dynamic::from),
    };
    ctx.frame().begin_cast(checked.id, checked.rank)?;
    let pool = unit
        .get::<Owner>()
        .map_or(Pool::Think, |controller| Pool::Player(controller.slot()));
    Ok(Some(Prepared {
        caster,
        pool,
        slot: casting.slot,
        target,
        on_cast: checked.ability.on_cast,
        cost: checked.cost,
        cooldown: checked.cooldown,
    }))
}

/// Runs the prepared cast's `on_cast`, which queues its effects in the frame.
fn run(batch: &mut ScriptBatch<'_>, ctx: &Ctx, prepared: &mut Prepared) -> Result<(), CallError> {
    let Some(script) = prepared.on_cast else {
        return Ok(());
    };
    let target = mem::take(&mut prepared.target);
    let caster = prepared.caster.clone();
    let args = (ctx.clone(), caster, target);
    batch
        .call(prepared.pool, script, Hook::OnCast, args)
        .map(drop)
        .map_err(CallError::from_script)
}

#[cfg(test)]
mod tests;
