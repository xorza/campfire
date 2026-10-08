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
use crate::scripts::call_start::CallStart;
use crate::units::action_id::ActionId;

use crate::actions::action_data::TogglePer;
use crate::actions::action_slots::{
    ActionCall, ActionSlot, ActionSlots, ChannelCall, ChannelStep, InProgress, OrderPhase,
    ResolvedCast,
};

use crate::actions::action_target::ActionTarget;
use crate::actions::purse::Purse;
use crate::areas::Areas;
use crate::combat::CombatSet;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;

use crate::actions::targets::Targets;
use crate::deliveries::Deliveries;
use crate::deliveries::delivering::Delivering;
use crate::players::player_resources::PlayerResources;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::{DashDelivery, ForcedMove};

use crate::projectiles::Projectiles;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;

use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;

use crate::stats::StatsSet;

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
            .and_then(|slot| book.get(slot.action?)?.channel_rule(slot.rank))
            .map_or(Ticks::ZERO, |rule| rule.tick)
    }

    /// A second at `rate`, the time a toggle pays its cost each.
    fn second(rate: TickRate) -> Ticks {
        rate.ticks(1000)
            .expect("a second counts in ticks at every rate")
    }

    /// Adds abilities to a match, on the core `Units` installs: in Hit, after attacks strike and
    /// before the tick's projectiles launch, due casts resolve: the delivery, the cost, the
    /// cooldown and the script's effects apply together, or none of them. A dash an instant cast
    /// starts is its delivery, whose end runs with the deliveries' hooks. A cast resolves in the
    /// script host; without the core's scripts, as on a client, a due cast of a unit it predicts
    /// only cools down, as the server's does.
    pub fn install(world: &mut World, schedule: &mut Schedule, _: &mut StateRegistry) {
        Deliveries::install(world, schedule);
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
    let slots = unit.get::<ActionSlots>();
    let channel = slots.and_then(|slots| slots.slot(slots.channeling()?));
    let group = channel.map_or(Block::Cast, |slot| {
        Inventory::group(unit.get::<Inventory>(), slot.kind)
    });
    let blocked = ForcedMove::blocks(unit.get::<UnitTags>(), forced, group);
    world.resource_scope(|world, book: Mut<'_, ActionBook>| {
        let mut slots = world
            .get_mut::<ActionSlots>(entity)
            .expect("a unit whose channel runs has slots");
        let tick = Abilities::channel_tick(&book, &slots);
        slots.step_channel(now, blocked, tick)
    })
}

/// Runs `hook` of the channel `call` of `caster`, the unit of `entity`, of its action at its rank,
/// as the action started, with the unit and, for `on_interrupt`, the target, when the action's
/// script defines it; a failed call applies nothing, and is recorded.
fn channel_call(
    batch: &mut ScriptBatch<'_>,
    ctx: &Ctx,
    now: Tick,
    caster: StableId,
    entity: Entity,
    call: ChannelCall,
    hook: Hook,
) {
    let ChannelCall {
        call: ActionCall { aim, start },
        action: id,
        rank,
    } = call;
    let world = batch.world();
    let owner = world.get::<Owner>(entity).map(|owner| owner.slot());
    let book = world.resource::<ActionBook>();
    let action = book.get(id).expect("a channel's action is in the book");
    let (Some(script), Some(handle)) = (action.hook(hook), ctx.view().unit(caster)) else {
        return;
    };
    let package = action.package;
    let start = CallStart {
        start: Some(start),
        ..CallStart::cast(id, rank, caster, package)
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
            Option<&Inventory>,
            Has<Dead>,
        ),
    >,
    mut on: Local<'_, Vec<(u8, ActionSlot)>>,
) {
    let now = tick.start();
    let second = Abilities::second(*rate);
    for (mut slots, mut pools, tags, inventory, dead) in &mut units {
        on.clear();
        on.extend(slots.indexed().filter(|(_, slot)| slot.toggle.is_some()));
        for &(at, slot) in &*on {
            let group = Inventory::group(inventory, slot.kind);
            if dead || UnitTags::properties_of(tags).blocks(group) {
                slots.toggle_off(at);
                continue;
            }
            let next = slot.toggle.expect("a toggle that is on");
            let toggle = slot
                .action
                .and_then(|action| book.get(action)?.toggle_rule(slot.rank))
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
/// range, starts, or for a charged action starts to charge; one whose target is beyond range, of a
/// unit that walks, walks in range first, its walk in place of any other, and stops there to start;
/// any other is dropped, and stops the walk it took. A unit its tags or a forced move keep from
/// casting keeps its order, and stands: a cast it started or walked in range for goes back to it,
/// and a charge ends, spending nothing. A charge its order released, or that is full, resolves.
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
            Option<&mut Destination>,
            Option<&Route>,
            Option<&Inventory>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (
        &position,
        &team,
        mut slots,
        pools,
        owner,
        body,
        tags,
        forced,
        mut destination,
        route,
        inventory,
    ) in &mut units
    {
        let Some(underway) = slots.in_progress() else {
            continue;
        };
        let kind = slots
            .slot(underway.slot())
            .expect("a slot the unit has")
            .kind;
        let blocked = ForcedMove::blocks(tags, forced, Inventory::group(inventory, kind));
        if let InProgress::Charge { .. } = underway {
            if blocked {
                slots.interrupt();
            } else {
                slots.release(now);
            }
            continue;
        }
        let Some(InProgress::Order { aim, phase }) = slots.in_progress() else {
            continue;
        };
        let slot = slots
            .slot(aim.slot)
            .expect("an order of a slot the unit has");
        let approached = phase == OrderPhase::Approaching;
        // An item sold in the tick of its order leaves its slot with no action to start.
        let Some(id) = slot.action else {
            drop_cast(&mut slots, destination.as_mut(), route, approached);
            continue;
        };
        let action = book.get(id).expect("a slot's action is in the book");
        if action.kind.kind() != ActionKind::Cast {
            continue;
        }
        if blocked {
            if approached {
                walk(destination.as_mut(), route, None);
            }
            slots.interrupt();
            continue;
        }
        if let OrderPhase::Started(_) = phase {
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
        let purse = Purse::of(pools, resources.as_deref(), owner);
        let attitude = |other| targets.attitude(team, other);
        let shape = Body::shape_of(body);
        let checked = book
            .check(now, &slots, purse, aim, attitude, |id| targets.living(id))
            .map(|mut checked| {
                checked.clamp(position, shape, &targets);
                checked
            });
        let Some(checked) = checked else {
            drop_cast(&mut slots, destination.as_mut(), route, approached);
            continue;
        };
        if !checked.in_range(position, shape, &targets) {
            match (destination.is_some(), checked.aimed_at(&targets)) {
                (true, Some(to)) => {
                    walk(destination.as_mut(), route, Some(to));
                    slots.approach();
                }
                _ => slots.stop(),
            }
            continue;
        }
        if approached {
            walk(destination.as_mut(), route, None);
        }
        slots.begin(&checked, position, now);
    }
}

/// Drops the cast ordered in `slots`, and stops the walk it took when it `approached`.
fn drop_cast(
    slots: &mut ActionSlots,
    destination: Option<&mut Mut<'_, Destination>>,
    route: Option<&Route>,
    approached: bool,
) {
    if approached {
        walk(destination, route, None);
    }
    slots.stop();
}

/// Walks the unit of `destination`, one that walks, to `to`, or stops it.
fn walk(
    destination: Option<&mut Mut<'_, Destination>>,
    route: Option<&Route>,
    to: Option<Position>,
) {
    if let Some(destination) = destination {
        Destination::walk_to(destination, route, to);
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
        let slots = unit.get::<ActionSlots>().expect("a due caster has slots");
        let due = slots.in_progress().expect("a due cast is under way");
        let kind = slots.slot(due.slot()).expect("a cast's slot").kind;
        let group = Inventory::group(unit.get::<Inventory>(), kind);
        let can_cast = !ForcedMove::blocks(unit.get::<UnitTags>(), forced, group);
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
            Option<&Inventory>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    let second = Abilities::second(*rate);
    for (&team, mut slots, pools, owner, tags, forced, inventory) in &mut casters {
        let Some(ActionCall {
            aim: casting,
            start,
        }) = slots
            .in_progress()
            .and_then(|underway| underway.cast_due(now))
        else {
            continue;
        };
        let kind = slots.slot(casting.slot).expect("a cast's slot").kind;
        if ForcedMove::blocks(tags, forced, Inventory::group(inventory, kind)) {
            slots.interrupt();
            continue;
        }
        let purse = Purse::of(pools, resources.as_deref(), owner);
        let living = |id| targets.living(id);
        let attitude = |other| targets.attitude(team, other);
        let resolved = book
            .check(now, &slots, purse, casting, attitude, living)
            .map(|checked| {
                checked.resolved(ActionCall {
                    aim: casting,
                    start,
                })
            });
        slots.finish_cast(now, second, resolved.as_ref());
    }
}

/// A cast ready to run: the caster as the script sees it, the pool its call draws from, its
/// action and rank, its target as the script sees it, its `on_resolve`, and how it resolves. Its
/// params wait in the frame.
#[derive(Debug)]
struct Prepared {
    caster: Unit,
    pool: Pool,
    action: ActionId,
    rank: u8,
    target: Dynamic,
    on_resolve: Option<ScriptId>,
    resolved: ResolvedCast,
}

/// Resolves one cast: its script runs, then its effects, cost, cooldown and slot's changes apply
/// together, or, when the cast no longer passes its checks or it fails, none of them, and it
/// only stops. Of an item's action, a use of its consumable is spent last, as one used up empties
/// its slot.
fn resolve(batch: &mut ScriptBatch<'_>, ctx: &Ctx, now: Tick, caster: StableId, entity: Entity) {
    let ran = prepare(batch.world(), ctx, now, caster, entity).and_then(|prepared| {
        let Some(mut prepared) = prepared else {
            return Ok(None);
        };
        run(batch, ctx, &mut prepared)?;
        apply(batch.world(), ctx, now, entity, &prepared);
        Ok(Some(prepared.resolved))
    });
    let resolved = ran.unwrap_or_else(|error| {
        batch.record(Some(caster), Hook::OnResolve, error);
        None
    });
    let world = batch.world();
    let second = Abilities::second(*world.resource::<TickRate>());
    world
        .get_mut::<ActionSlots>(entity)
        .expect("a caster has slots")
        .finish_cast(now, second, resolved.as_ref());
    if let Some(resolved) = &resolved
        && world.contains_resource::<ItemBook>()
    {
        world.resource_scope(|world, book: Mut<'_, ItemBook>| {
            let mut caster = world.entity_mut(entity);
            let carried = caster.get_components_mut::<(&mut Inventory, &mut ActionSlots)>();
            if let Ok((mut inventory, mut slots)) = carried {
                inventory.spend_use(&book, &mut slots, resolved.call.aim.slot);
            }
        });
    }
}

/// Applies a cast that ran: its delivery's launches, then the effects it queued in `frame` and
/// its handle writes, and its cost in pools.
fn apply(world: &mut World, ctx: &Ctx, now: Tick, entity: Entity, prepared: &Prepared) {
    let from = *world.get::<Position>(entity).expect("a caster stands");
    let ActionCall { aim, start } = prepared.resolved.call;
    let by = Delivering {
        source: prepared.caster.id,
        action: prepared.action,
        rank: prepared.rank,
        start: Some(start),
        launch: None,
    };
    let book = world.resource::<ActionBook>();
    match book.get(by.action).and_then(|action| action.delivery) {
        Some(Delivery {
            unit_type,
            shape: DeliveryShape::Projectile { fan, .. },
        }) => {
            Projectiles::deliver(world, by, from, unit_type, fan, aim.target);
        }
        Some(Delivery {
            unit_type,
            shape: DeliveryShape::Area,
        }) => Areas::deliver(world, by, from, unit_type, aim.target),
        None => {}
    }
    ctx.apply(world, now);
    // The player's resources were paid in the call's frame, before its script ran, so a failed
    // call pays nothing and the script cannot spend what the cost took.
    if let Some(mut pools) = world.get_mut::<Pools>(entity) {
        pools.pay(&prepared.resolved.values.cost);
    }
}

/// The cast of `entity` checked again, and its params at its rank put in the frame, with its
/// delivery for a dash it starts when it delivers at once; `None` when it no longer passes its
/// checks, or its caster is no unit the view read.
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
    let owner = unit.get::<Owner>();
    let purse = Purse::of(
        unit.get::<Pools>(),
        world.get_resource::<PlayerResources>(),
        owner,
    );
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
            dash_delivers: checked
                .action
                .delivery
                .is_none()
                .then_some(DashDelivery::new(
                    caster.id,
                    checked.id,
                    checked.rank,
                    Some(start),
                )),
            ..CallStart::cast(checked.id, checked.rank, caster.id, package)
        },
    )?;
    // The check let a cost in player resources pass only for a unit a player owns, in a match
    // that keeps them.
    let resource_cost = checked.action.resource_cost(checked.rank);
    if !resource_cost.is_empty() {
        let owner = owner.expect("a unit that pays player resources has an owner");
        let resources = frame.resources_mut();
        let resources = resources.expect("a match that takes player resources keeps them");
        resources.pay(owner.slot(), resource_cost);
    }
    drop(frame);
    let pool = owner.map_or(Pool::Think, |owner| Pool::Player(owner.slot()));
    Ok(Some(Prepared {
        caster,
        pool,
        action: checked.id,
        rank: checked.rank,
        target,
        on_resolve: checked.action.hook(Hook::OnResolve),
        resolved: checked.resolved(ActionCall {
            aim: casting,
            start,
        }),
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
        prepared.resolved.call.aim.target,
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
