use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_script::ScriptId;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::actions::effect_lists::{EffectLists, ListsOf};

use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::delivery::{Delivery, DeliveryShape};
use crate::actions::rank_values::{ChannelRule, ChargeRule};
use crate::scripts::call_start::CallStart;
use crate::units::action_id::ActionId;

use crate::actions::action_data::TogglePer;
use crate::actions::action_slots::{
    ActionCall, ActionSlot, ActionSlots, ChannelStep, InProgress, SlotAim,
};
use crate::values::action_start::ActionStart;

use crate::actions::action_target::ActionTarget;
use crate::actions::purse::{Payer, Purse};
use crate::areas::Areas;
use crate::combat::CombatSet;

use crate::actions::targets::Targets;
use crate::deliveries::delivering::Delivering;
use crate::players::player_resources::PlayerResources;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::ForcedMove;

use crate::projectiles::Projectiles;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;

use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;

use crate::stats::StatsSet;
use crate::stats::pool_cost::PoolCost;

use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::owner::Owner;

use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;

pub(crate) mod abilities_api;
pub(crate) mod abilities_effect;

/// The `abilities` capability: abilities in slots, cast through their checks, with the effect a
/// script describes.
#[derive(Debug)]
pub struct Abilities;

/// The systems of `abilities`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AbilitiesSet {
    /// In `SimSet::Inputs`, before the holds follow: toggles pay their seconds or turn off.
    Toggles,
}

impl Abilities {
    /// `target` as a script reads it: a living unit's handle, a point, or `()` for none or a
    /// unit no longer living.
    fn target(view: &View, target: ActionTarget) -> Dynamic {
        match target {
            ActionTarget::None => Dynamic::UNIT,
            ActionTarget::Unit(id) => view
                .living(id)
                .and_then(|_| view.unit(id))
                .map_or(Dynamic::UNIT, Dynamic::from),
            ActionTarget::Point(at) => Dynamic::from(at),
        }
    }

    /// The time between the ticks of the channel `slots` runs, if one runs.
    fn channel_tick(book: &ActionBook, slots: &ActionSlots) -> Ticks {
        slots
            .channeling()
            .and_then(|slot| slots.slot(slot))
            .and_then(|slot| book.get(slot.action)?.channel_rule(slot.rank))
            .map_or(Ticks::ZERO, |rule| rule.tick)
    }

    /// A second at `rate`, the time a toggle pays its cost each.
    fn second(rate: TickRate) -> Ticks {
        rate.ticks(1000)
            .expect("a second counts in ticks at every rate")
    }

    /// Adds abilities to a match, on the core `Units` installs: in Hit, after attacks strike and
    /// before the tick's projectiles launch, due casts resolve: the delivery, the cost, the
    /// cooldown and the script's effects apply together, or none of them. A cast resolves in the
    /// script host; without the core's scripts, as on a client, a due cast of a unit it predicts
    /// only cools down, as the server's does.
    pub fn install(world: &mut World, schedule: &mut Schedule, _: &mut StateRegistry) {
        schedule.add_systems(start_casts.in_set(SimSet::Act).in_set(ActionsSet::Start));
        if !world.contains_non_send::<Ctx>() {
            schedule.add_systems(
                (predict_casts, predict_channels)
                    .chain()
                    .in_set(SimSet::Hit)
                    .after(CombatSet::Fire)
                    .before(CombatSet::Launch),
            );
            return;
        }
        schedule.add_systems((
            (resolve_casts, run_channels)
                .chain()
                .in_set(SimSet::Hit)
                .after(CombatSet::Fire)
                .before(CombatSet::Launch),
            run_toggles
                .in_set(SimSet::Inputs)
                .in_set(AbilitiesSet::Toggles)
                .after(StatsSet::Regenerate)
                .after(CombatSet::Respawn)
                .before(ActionsSet::HoldAtInputs),
        ));
    }
}

/// Runs each unit's channel in Hit, in the order of its stable id, after the casts resolve: a cut
/// one's `on_interrupt`, then a due tick's `on_channel_tick`, each a call of the channel's action
/// at its rank from the unit, in its player's pool; and one at its end ends. A unit whose tags
/// keep it from casting has its channel cut. A channel's calls share one snapshot of the units,
/// read as the batch begins.
fn run_channels(
    world: &mut World,
    units: &mut QueryState<(Entity, &StableId, &ActionSlots)>,
    (mut order, mut due): (Local<'_, Ordered>, Local<'_, Vec<Keyed>>),
) {
    let now = world.resource::<SimTick>().start();
    let running = units
        .iter(world)
        .filter(|(.., slots)| slots.channel_due())
        .map(|(entity, &id, _)| Keyed { id, entity });
    due.clear();
    due.extend_from_slice(order.sort(running));
    if due.is_empty() {
        return;
    }
    let ctx = world.non_send::<Ctx>().clone();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &Keyed { id, entity } in &*due {
            let step = step_channel(batch.world(), now, entity);
            for (call, hook) in [
                (step.interrupted, Hook::OnInterrupt),
                (step.ticked, Hook::OnChannelTick),
            ] {
                if let Some(call) = call {
                    channel_call(batch, &ctx, now, id, entity, call, hook);
                }
            }
        }
    });
}

/// Runs the channel of `entity` at `now`, as `run_channels` does, and gives back the hooks due.
fn step_channel(world: &mut World, now: Tick, entity: Entity) -> ChannelStep {
    let unit = world.entity(entity);
    let forced = unit.contains::<ForcedMove>();
    let blocked = ForcedMove::blocks(unit.get::<UnitTags>(), forced, Block::Cast);
    world.resource_scope(|world, book: Mut<'_, ActionBook>| {
        let mut slots = world
            .get_mut::<ActionSlots>(entity)
            .expect("a unit whose channel runs has slots");
        let tick = Abilities::channel_tick(&book, &slots);
        slots.step_channel(now, blocked, tick)
    })
}

/// Runs `hook` of the channel `call` of `caster`, the unit of `entity`, at its action's rank, as
/// the action started, with the unit and, for `on_interrupt`, the target, when the action's script
/// defines it; a failed call applies nothing, and is recorded.
fn channel_call(
    batch: &mut ScriptBatch<'_>,
    ctx: &Ctx,
    now: Tick,
    caster: StableId,
    entity: Entity,
    call: ActionCall,
    hook: Hook,
) {
    let ActionCall { aim, start } = call;
    let world = batch.world();
    let unit = world.entity(entity);
    let slot = unit
        .get::<ActionSlots>()
        .and_then(|slots| slots.slot(aim.slot))
        .expect("a channel's slot");
    let owner = unit.get::<Owner>().map(|owner| owner.slot());
    let book = world.resource::<ActionBook>();
    let action = book
        .get(slot.action)
        .expect("a slot's action is in the book");
    let (Some(script), Some(handle)) = (action.hook(hook), ctx.view().unit(caster)) else {
        return;
    };
    let package = action.package;
    let start = CallStart {
        start: Some(start),
        ..CallStart::cast(slot.action, slot.rank, caster, package)
    };
    let begun = ctx.frame().begin(world, start);
    let outcome = begun.and_then(|()| {
        let pool = owner.map_or(Pool::Think, Pool::Player);
        let called = match hook {
            Hook::OnInterrupt => {
                let target = Abilities::target(ctx.view(), aim.target);
                batch.call(pool, script, hook, (ctx.clone(), handle, target))
            }
            _ => batch.call(pool, script, hook, (ctx.clone(), handle)),
        };
        called.map(drop).map_err(CallError::from_script)
    });
    match outcome {
        Ok(()) => ctx.apply(batch.world(), now),
        Err(error) => batch.record(Some(caster), hook, error),
    }
}

/// Runs each channel of a unit a client predicts as the server does, with no call: a cut one's
/// record clears, and one at its end ends, as the hooks' effects come from the server.
fn predict_channels(
    world: &mut World,
    units: &mut QueryState<(Entity, &ActionSlots)>,
    mut due: Local<'_, Vec<Entity>>,
) {
    let now = world.resource::<SimTick>().start();
    due.clear();
    due.extend(
        units
            .iter(world)
            .filter(|(_, slots)| slots.channel_due())
            .map(|(entity, _)| entity),
    );
    for &entity in &*due {
        step_channel(world, now, entity);
    }
}

/// Keeps each toggle that is on, as each tick starts, before the holds follow: death or a tag that
/// blocks casting turns it off; one that costs a second pays at each whole second from when it
/// turned on, and one its pools cannot pay turns off. The server alone runs it, as a client's
/// pools come from the server.
fn run_toggles(
    (tick, rate, book): (Res<'_, SimTick>, Res<'_, TickRate>, Res<'_, ActionBook>),
    mut units: Query<
        '_,
        '_,
        (
            &mut ActionSlots,
            Option<&mut Pools>,
            Option<&UnitTags>,
            Has<Dead>,
        ),
    >,
    mut on: Local<'_, Vec<(u8, ActionSlot)>>,
) {
    let now = tick.start();
    let second = Abilities::second(*rate);
    for (mut slots, mut pools, tags, dead) in &mut units {
        on.clear();
        on.extend(
            (0..)
                .zip(slots.iter())
                .filter(|(_, slot)| slot.toggle.is_some()),
        );
        let stops = dead || UnitTags::effects_of(tags).blocks(Block::Cast);
        for &(at, slot) in &*on {
            if stops {
                slots.toggle_off(at);
                continue;
            }
            let next = slot.toggle.expect("a toggle that is on");
            let toggle = book
                .get(slot.action)
                .and_then(|action| action.toggle_rule(slot.rank))
                .expect("a toggle that is on has its rule");
            if toggle.per != TogglePer::Second || next > now {
                continue;
            }
            match pools.as_deref_mut() {
                Some(pools) if pools.affords(&toggle.cost) => {
                    pools.pay(&toggle.cost);
                    slots.toggle_on(at, next.after(second));
                }
                _ => slots.toggle_off(at),
            }
        }
    }
}

/// Starts each cast a unit was ordered, in Act: one that passes its checks, its target within
/// range, starts, or for a charged action starts to charge, and any other is dropped. A unit its
/// tags keep from casting keeps its order: a cast it started goes back to it, and a charge ends,
/// spending nothing. A charge its order released, or that is full, resolves.
fn start_casts(
    tick: Res<'_, SimTick>,
    book: Res<'_, ActionBook>,
    resources: Option<Res<'_, PlayerResources>>,
    targets: Targets<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &mut ActionSlots,
            Option<&Pools>,
            Option<&Owner>,
            Option<&Body>,
            Option<&UnitTags>,
            Has<ForcedMove>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, mut slots, pools, owner, body, tags, forced) in &mut units {
        let blocked = ForcedMove::blocks(tags, forced, Block::Cast);
        if let Some(InProgress::Charge { .. }) = slots.in_progress() {
            if blocked {
                slots.interrupt();
            } else {
                slots.release(now);
            }
            continue;
        }
        let Some(InProgress::Order { aim, started }) = slots.in_progress() else {
            continue;
        };
        let slot = slots
            .slot(aim.slot)
            .expect("an order of a slot the unit has");
        let action = book
            .get(slot.action)
            .expect("a slot's action is in the book");
        if action.kind.kind() != ActionKind::Cast {
            continue;
        }
        if blocked {
            if started.is_some() {
                slots.interrupt();
            }
            continue;
        }
        if started.is_some() {
            continue;
        }
        if slot.toggle.is_some() {
            if now >= slot.ready_at {
                slots.toggle_off(aim.slot);
                slots.cool_down(aim.slot, now.after(action.values(slot.rank).cooldown));
            }
            slots.stop();
            continue;
        }
        let purse = Purse {
            pools,
            resources: resources.as_deref(),
            owner: owner.map(|owner| owner.slot()),
        };
        let attitude = |other| targets.attitude(team, other);
        let radius = Body::radius_of(body);
        let started = book
            .check(now, &slots, purse, aim, attitude, |id| targets.living(id))
            .map(|mut checked| {
                checked.clamp(position, radius, &targets);
                checked
            })
            .filter(|checked| checked.in_range(position, radius, &targets))
            .map(|checked| (checked.values, checked.target));
        match started {
            Some((values, target)) => {
                if let Some(most) = values.charge {
                    slots.charge(target, position, now, now.after(most));
                } else {
                    let start = ActionStart {
                        origin: position,
                        charge: None,
                    };
                    slots.start(now.after(values.windup), target, start);
                }
            }
            None => slots.stop(),
        }
    }
}

/// Resolves the casts due this tick, in the order of their caster's stable id. Their calls share
/// one snapshot of the living units, read as the batch begins, so no call sees what an earlier one
/// changed, at once or in Resolve. A due cast whose caster's tags keep it from casting goes back
/// to its order instead.
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
                .and_then(|underway| underway.cast_due(now))
                .is_some()
        })
        .map(|(entity, &id, _)| Keyed { id, entity });
    due.clear();
    due.extend_from_slice(order.sort(resolving));
    due.retain(|&Keyed { entity, .. }| {
        let unit = world.entity(entity);
        let forced = unit.contains::<ForcedMove>();
        let can_cast = !ForcedMove::blocks(unit.get::<UnitTags>(), forced, Block::Cast);
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

/// Resolves each due cast of a unit a client predicts as the server does when the cast's script
/// runs: one that passes its checks again cools down, and every due cast stops; one whose caster's
/// tags keep it from casting goes back to its order. Its cost and its effects come from the
/// server.
fn predict_casts(
    (tick, rate): (Res<'_, SimTick>, Res<'_, TickRate>),
    book: Res<'_, ActionBook>,
    resources: Option<Res<'_, PlayerResources>>,
    targets: Targets<'_, '_>,
    mut casters: Query<
        '_,
        '_,
        (
            &Team,
            &mut ActionSlots,
            Option<&Pools>,
            Option<&Owner>,
            Option<&UnitTags>,
            Has<ForcedMove>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    let second = Abilities::second(*rate);
    for (&team, mut slots, pools, owner, tags, forced) in &mut casters {
        let Some(ActionCall {
            aim: casting,
            start,
        }) = slots
            .in_progress()
            .and_then(|underway| underway.cast_due(now))
        else {
            continue;
        };
        if ForcedMove::blocks(tags, forced, Block::Cast) {
            slots.interrupt();
            continue;
        }
        let purse = Purse {
            pools,
            resources: resources.as_deref(),
            owner: owner.map(|owner| owner.slot()),
        };
        let living = |id| targets.living(id);
        let attitude = |other| targets.attitude(team, other);
        let checked = book
            .check(now, &slots, purse, casting, attitude, living)
            .map(|checked| (checked.values, checked.target));
        slots.stop();
        if let Some((values, target)) = checked {
            slots.spend(casting.slot, now, values.cooldown, values.charges);
            if values.toggle.is_some() {
                slots.toggle_on(casting.slot, now.after(second));
            }
            if let Some(rule) = values.channel {
                let aim = SlotAim {
                    slot: casting.slot,
                    target,
                };
                slots.channel(aim, now.after(Ticks::new(1)), rule, start);
            }
        }
    }
}

/// A cast ready to run: the caster as the script sees it, the pool its call draws from, its
/// slot, action and rank, its target as it aimed and as the script sees it, its `on_resolve`,
/// and its cost, cooldown and charges. Its params wait in the frame.
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
    charges: Option<ChargeRule>,
    /// Whether it turns its toggle on.
    toggles: bool,
    /// Its channel, which starts the tick after it resolves.
    channel: Option<ChannelRule>,
    /// How it started, which each of its hooks reads.
    start: ActionStart,
}

/// Resolves one cast: its script runs, then its effects, cost and cooldown apply together, or,
/// when the cast no longer passes its checks or it fails, none of them.
fn resolve(batch: &mut ScriptBatch<'_>, ctx: &Ctx, now: Tick, caster: StableId, entity: Entity) {
    let prepared = prepare(batch.world(), ctx, now, caster, entity);
    let mut channel = None;
    let outcome = match prepared {
        Ok(None) => Ok(()),
        Ok(Some(mut prepared)) => run(batch, ctx, &mut prepared).map(|()| {
            apply(batch.world(), ctx, now, entity, &prepared);
            let aim = SlotAim {
                slot: prepared.slot,
                target: prepared.aim,
            };
            channel = prepared.channel.map(|rule| (aim, rule, prepared.start));
        }),
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        batch.record(Some(caster), Hook::OnResolve, error);
    }
    let mut slots = batch
        .world()
        .get_mut::<ActionSlots>(entity)
        .expect("a caster has slots");
    slots.stop();
    if let Some((aim, rule, start)) = channel {
        slots.channel(aim, now.after(Ticks::new(1)), rule, start);
    }
}

/// Applies a cast that ran: its delivery's launches, then the effects it queued in `frame` and
/// its handle writes, its cost and its cooldown.
fn apply(world: &mut World, ctx: &Ctx, now: Tick, entity: Entity, prepared: &Prepared) {
    let from = *world.get::<Position>(entity).expect("a caster stands");
    let by = Delivering {
        source: prepared.caster.id,
        action: prepared.action,
        rank: prepared.rank,
        start: Some(prepared.start),
        launch: None,
    };
    let book = world.resource::<ActionBook>();
    match book.get(by.action).and_then(|action| action.delivery) {
        Some(Delivery {
            unit_type,
            shape: DeliveryShape::Projectile { fan, .. },
        }) => {
            Projectiles::deliver(world, by, from, unit_type, fan, prepared.aim);
        }
        Some(Delivery {
            unit_type,
            shape: DeliveryShape::Area,
        }) => Areas::deliver(world, by, from, unit_type, prepared.aim),
        None => {}
    }
    ctx.apply(world, now);
    // The player's resources were paid in the call's frame, before its script ran, so a failed
    // call pays nothing and the script cannot spend what the cost took.
    let payer = Payer {
        pools: world.get_mut::<Pools>(entity).map(Mut::into_inner),
        resources: None,
        owner: None,
    };
    payer.pay(&prepared.cost, &[]);
    world
        .get_mut::<ActionSlots>(entity)
        .expect("a caster has slots")
        .spend(prepared.slot, now, prepared.cooldown, prepared.charges);
    if prepared.toggles {
        let second = Abilities::second(*world.resource::<TickRate>());
        world
            .get_mut::<ActionSlots>(entity)
            .expect("a caster has slots")
            .toggle_on(prepared.slot, now.after(second));
    }
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
    let ActionCall {
        aim: casting,
        start,
    } = slots
        .in_progress()
        .and_then(|underway| underway.cast_due(now))
        .expect("a due caster casts");
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
    let target = Abilities::target(view, casting.target);
    let mut frame = ctx.frame();
    let package = checked.action.package;
    frame.begin(
        world,
        CallStart {
            start: Some(start),
            ..CallStart::cast(checked.id, checked.rank, caster.id, package)
        },
    )?;
    let resource_cost = checked.action.resource_cost(checked.rank);
    if !resource_cost.is_empty() {
        let payer = Payer {
            pools: None,
            resources: frame.resources_mut(),
            owner,
        };
        payer.pay(&PoolCost::default(), resource_cost);
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
        charges: checked.values.charges,
        toggles: checked.values.toggle.is_some(),
        channel: checked.values.channel,
        start,
    }))
}

/// Queues the prepared cast's `on_resolve` list in the frame, to the unit it aimed at, then runs
/// its script's `on_resolve`, which queues its own effects after it.
fn run(batch: &mut ScriptBatch<'_>, ctx: &Ctx, prepared: &mut Prepared) -> Result<(), CallError> {
    EffectLists::queue(
        batch.world(),
        ListsOf::Action(prepared.action),
        Hook::OnResolve,
        &mut ctx.frame(),
        ctx.view(),
        prepared.aim.unit(),
    )?;
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

#[cfg(test)]
mod tests;
