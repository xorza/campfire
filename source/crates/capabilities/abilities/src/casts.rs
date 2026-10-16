use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_actions::{
    ActionBook, ActionCall, ActionCheck, ActionKind, ActionProgress, ActionSlots, CastSpends,
    Caster, EffectListBook, InProgress, ListCall, ListHook, ListsOf, OrderPhase, Purse,
    ResolvedCast, SlotTimes, Targets, UseGroup,
};
use campfire_common::{PlayerSlot, Tick};
use campfire_deliveries::{Deliverers, Delivering, Launcher};
use campfire_math::Vec3;
use campfire_players::{PlayerResources, PlayerRows, PlayersCall, PlayersWorld};
use campfire_script::{
    CallError, CallStart, Ctx, Hook, HookCall, Pool, ScriptBatch, ScriptId, ScriptValue,
    ScriptsWorld, Unit,
};
use campfire_sim::{Keyed, Ordered, Position, SimTick, StableId, TickClock, TickRate};
use campfire_stats::{PoolBook, StatsWorld, UnitPools, UnitPoolsReadOnly};
use campfire_units::{ArrivedShort, Body, Dead, Destination, ForcedMove, Owner};
use campfire_values::{ActionId, DashDelivery, Motion, Rank, Team, UnitTags};

use crate::Abilities;

/// A cast ready to run: how its call starts, the caster as the script sees it, and its player
/// if any, whose pool its call draws from, its action and rank, its target as the script sees
/// it, its `on_resolve`, and how it resolves.
#[derive(Debug)]
struct Prepared {
    start: CallStart,
    caster: Unit,
    owner: Option<PlayerSlot>,
    action: ActionId,
    rank: Rank,
    target: ScriptValue,
    on_resolve: Option<ScriptId>,
    resolved: ResolvedCast,
}

/// The casts units are ordered: their start in Act, and in Hit their resolve, on a client
/// predicted.
#[derive(Debug)]
pub struct Casts;

impl Casts {
    /// Starts each cast or use a unit was ordered, in Act: one that passes its checks, its target
    /// within range, starts, or for a charged action starts to charge; one whose target is beyond
    /// range, of a unit that walks, walks in range first, its walk in place of any other, and stops
    /// there to start; any other is dropped, and stops the walk it took. A unit its tags or a
    /// forced move keep from casting, or that lives no more, keeps its order, and stands: a cast
    /// it started or walked in range for goes back to it, and a charge ends, spending nothing. A
    /// charge its order released, or that is full, resolves.
    pub(super) fn start_casts(
        tick: Res<'_, SimTick>,
        book: Res<'_, ActionBook>,
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        targets: Targets<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                &Position,
                &Team,
                (&ActionSlots, &mut ActionProgress),
                &mut SlotTimes,
                UnitPoolsReadOnly,
                Option<&Owner>,
                Option<&Body>,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&mut Destination>,
                Option<&ArrivedShort>,
                Option<&UseGroup>,
            ),
            Without<Dead>,
        >,
    ) {
        let now = tick.start();
        let life = life.life();
        for (
            &position,
            &team,
            (slots, mut progress),
            mut times,
            pools,
            owner,
            body,
            tags,
            forced,
            mut destination,
            arrived,
            used,
        ) in &mut units
        {
            let Some(underway) = progress.in_progress() else {
                continue;
            };
            let underway_slot = slots.slot(underway.slot()).expect("a slot the unit has");
            let blocked = Caster::of(tags, Motion::of(forced), used)
                .blocked(Some(underway_slot), &book)
                || !pools.lives();
            if let InProgress::Charge { .. } = underway {
                if blocked {
                    progress.interrupt();
                } else {
                    progress.release(now);
                }
                continue;
            }
            let Some(InProgress::Order { aim, phase }) = progress.in_progress() else {
                continue;
            };
            let slot = slots
                .slot(aim.slot)
                .expect("an order of a slot the unit has");
            let approached = phase == OrderPhase::Approaching;
            // An item sold in the tick of its order leaves its slot with no action to start.
            let Some(id) = slot.action else {
                Self::drop_cast(&mut progress, destination.as_mut(), arrived, approached);
                continue;
            };
            let action = book.get(id).expect("a slot's action is in the book");
            if !matches!(action.kind.kind(), ActionKind::Cast | ActionKind::Use) {
                continue;
            }
            if blocked {
                if approached {
                    Self::walk(destination.as_mut(), arrived, None);
                }
                progress.interrupt();
                continue;
            }
            if let OrderPhase::Started(_) = phase {
                continue;
            }
            let time = times.slot(aim.slot).expect("a slot the unit has");
            if time.toggle.is_some() {
                if now >= time.ready_at {
                    times.toggle_off(aim.slot);
                    let rank = slot.rank.expect("a slot whose toggle is on is learned");
                    times.cool_down(aim.slot, now.after(action.values(rank).cooldown));
                }
                progress.stop();
                continue;
            }
            let purse = Purse::of(pools.of(life), resources.of(owner));
            let relation = |other| targets.relation(team, other);
            let shape = Body::shape_of(body);
            let checked = times
                .checked_action(ActionCheck {
                    book: &book,
                    now,
                    slots,
                    purse,
                    aim,
                    relation,
                    living: |id| targets.living(id),
                })
                .map(|mut checked| {
                    checked.clamp(position.get(), shape, &targets);
                    checked
                });
            let Some(checked) = checked else {
                Self::drop_cast(&mut progress, destination.as_mut(), arrived, approached);
                continue;
            };
            if !checked.in_range(position.get(), body, &targets) {
                match (destination.is_some(), checked.aimed_at(&targets)) {
                    (true, Some(to)) => {
                        Self::walk(destination.as_mut(), arrived, Some(to));
                        progress.approach();
                    }
                    _ => progress.stop(),
                }
                continue;
            }
            if approached {
                Self::walk(destination.as_mut(), arrived, None);
            }
            progress.begin(&checked, position.get(), now);
        }
    }

    /// Drops the cast ordered in `progress`, and stops the walk it took when it `approached`.
    fn drop_cast(
        progress: &mut ActionProgress,
        destination: Option<&mut Mut<'_, Destination>>,
        arrived: Option<&ArrivedShort>,
        approached: bool,
    ) {
        if approached {
            Self::walk(destination, arrived, None);
        }
        progress.stop();
    }

    /// Walks the unit of `destination`, one that walks, to `to`, or stops it.
    fn walk(
        destination: Option<&mut Mut<'_, Destination>>,
        arrived: Option<&ArrivedShort>,
        to: Option<Vec3>,
    ) {
        if let Some(destination) = destination {
            Destination::walk_to(destination, arrived, to);
        }
    }

    /// Resolves the casts due this tick, in the order of their caster's stable id. Their calls
    /// share one snapshot of the living units, read as the batch begins, so no call sees what an
    /// earlier one changed, at once or in Resolve. A due cast whose caster's tags keep it from
    /// casting, or that lives no more, goes back to its order instead.
    pub(super) fn resolve_casts(
        world: &mut World,
        casters: &mut QueryState<(Entity, &StableId, &ActionProgress), Without<Dead>>,
        (mut order, mut due): (Local<'_, Ordered>, Local<'_, Vec<Keyed>>),
    ) {
        let now = world.resource::<SimTick>().start();
        let resolving = casters
            .iter(world)
            .filter(|(.., progress)| {
                progress
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
            let progress = unit
                .get::<ActionProgress>()
                .expect("a due caster has progress");
            let due = progress.in_progress().expect("a due cast is under way");
            let slot = slots.slot(due.slot()).expect("a cast's slot");
            let used = unit.get::<UseGroup>();
            let book = world.resource::<ActionBook>();
            let can_cast = world.lives(entity)
                && !Caster::of(unit.get::<UnitTags>(), Motion::of(forced), used)
                    .blocked(Some(slot), book);
            if !can_cast {
                world
                    .get_mut::<ActionProgress>(entity)
                    .expect("a due caster has progress")
                    .interrupt();
            }
            can_cast
        });
        if due.is_empty() {
            return;
        }
        world.script_batch(|batch| {
            for &Keyed { id: caster, entity } in &*due {
                Self::resolve(batch, now, caster, entity);
            }
        });
    }

    /// Resolves each due cast of a unit a client predicts as the server does when the cast's script
    /// runs: one that passes its checks again cools down, and every due cast stops; one whose
    /// caster's tags keep it from casting, or that lives no more, goes back to its order. Its
    /// cost and its effects come from the server.
    pub(super) fn predict_casts(
        clock: TickClock<'_>,
        book: Res<'_, ActionBook>,
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        targets: Targets<'_, '_>,
        mut casters: Query<
            '_,
            '_,
            (
                &Team,
                (&ActionSlots, &mut ActionProgress),
                &mut SlotTimes,
                UnitPoolsReadOnly,
                Option<&Owner>,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&UseGroup>,
            ),
            Without<Dead>,
        >,
    ) {
        let now = clock.start();
        let life = life.life();
        let second = Abilities::second(clock.rate());
        for (&team, (slots, mut progress), mut times, pools, owner, tags, forced, used) in
            &mut casters
        {
            let Some(ActionCall {
                aim: casting,
                start,
            }) = progress
                .in_progress()
                .and_then(|underway| underway.cast_due(now))
            else {
                continue;
            };
            let slot = slots.slot(casting.slot).expect("a cast's slot");
            if Caster::of(tags, Motion::of(forced), used).blocked(Some(slot), &book)
                || !pools.lives()
            {
                progress.interrupt();
                continue;
            }
            let purse = Purse::of(pools.of(life), resources.of(owner));
            let living = |id| targets.living(id);
            let relation = |other| targets.relation(team, other);
            let resolved = times
                .checked_action(ActionCheck {
                    book: &book,
                    now,
                    slots,
                    purse,
                    aim: casting,
                    relation,
                    living,
                })
                .map(|checked| {
                    checked.resolved(ActionCall {
                        aim: casting,
                        start,
                    })
                });
            progress.finish_cast(slots, now, resolved.as_ref());
            if let Some(resolved) = &resolved {
                times.spend_cast(resolved, now, second);
            }
        }
    }

    /// Resolves one cast: its script runs, then its effects, cost, cooldown and slot's changes
    /// apply together, or, when the cast no longer passes its checks or it fails, none of them, and
    /// it only stops. Of an item's action, a use of its consumable is spent last, as one used up
    /// empties its slot.
    fn resolve(batch: &mut ScriptBatch<'_>, now: Tick, caster: StableId, entity: Entity) {
        let ctx = &batch.ctx().clone();
        let prepared = Self::prepare(batch.world(), ctx, now, caster, entity);
        let resolved = prepared.and_then(|mut prepared| {
            let ran = batch.run(prepared.start, |batch| {
                Self::pay_resources(batch.world(), ctx, &prepared);
                Self::run(batch, ctx, &mut prepared)?;
                Self::launch(batch.world(), entity, &prepared);
                Ok(())
            });
            match ran {
                Ok(()) => {
                    Self::pay_pools(batch.world(), entity, &prepared);
                    Some(prepared.resolved)
                }
                Err(error) => {
                    batch.record(Some(caster), Hook::OnResolve, error);
                    None
                }
            }
        });
        let world = batch.world();
        let second = Abilities::second(*world.resource::<TickRate>());
        let [mut unit] = world.entity_mut([entity]);
        let (slots, mut progress) = unit
            .get_components_mut::<(&ActionSlots, &mut ActionProgress)>()
            .expect("a caster has slots");
        progress.finish_cast(slots, now, resolved.as_ref());
        if let Some(resolved) = &resolved {
            world
                .get_mut::<SlotTimes>(entity)
                .expect("a caster has slot times")
                .spend_cast(resolved, now, second);
            CastSpends::spend(world, entity, resolved.call.aim.slot);
        }
    }

    /// Pays the cast's cost in player resources in the call's frame, before its script runs, so a
    /// failed call pays nothing and the script cannot spend what the cost took. The check let a
    /// cost in player resources pass only for a unit a player owns, in a match that keeps them.
    fn pay_resources(world: &World, ctx: &Ctx, prepared: &Prepared) {
        let book = world.resource::<ActionBook>();
        let action = book
            .get(prepared.action)
            .expect("a cast's action is in the book");
        let cost = action.resource_cost(prepared.rank);
        if cost.is_empty() {
            return;
        }
        let owner = prepared
            .owner
            .expect("a unit that pays player resources has an owner");
        let mut frame = ctx.frame();
        let players = frame
            .part_mut::<PlayersCall>()
            .expect("every call holds the players'");
        let resources = players.resources_mut(owner);
        resources
            .expect("a match that takes player resources keeps them")
            .pay(cost);
    }

    /// Launches the delivery of a cast that ran, before the effects it queued apply.
    fn launch(world: &mut World, entity: Entity, prepared: &Prepared) {
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
        if let Some(delivery) = book.get(by.action).and_then(|action| action.delivery) {
            let deliver = world.resource::<Deliverers>().of(delivery.shape);
            deliver(world, Launcher { by, delivery }, from.get(), aim.target);
        }
    }

    /// Pays the cost in pools of a cast that ran, after its effects applied.
    fn pay_pools(world: &mut World, entity: Entity, prepared: &Prepared) {
        let life = world.life_pool();
        let mut unit = world.entity_mut(entity);
        let pools = unit.get_components_mut::<UnitPools>();
        let mut pools = pools.expect("a caster's pools").of(life);
        pools.pay(&prepared.resolved.values.cost);
    }

    /// The cast of `entity` checked again, its call to start at its rank, with its delivery for a
    /// dash it starts when it delivers at once; `None` when it no longer passes its checks, or its
    /// caster is no unit the view read.
    fn prepare(
        world: &World,
        ctx: &Ctx,
        now: Tick,
        caster: StableId,
        entity: Entity,
    ) -> Option<Prepared> {
        let view = ctx.view();
        let caster = view.unit(caster)?;
        let unit = world.entity(entity);
        let slots = unit.get::<ActionSlots>().expect("a due caster has slots");
        let progress = unit
            .get::<ActionProgress>()
            .expect("a due caster has progress");
        let times = unit
            .get::<SlotTimes>()
            .expect("a due caster has slot times");
        let ActionCall {
            aim: casting,
            start,
        } = progress
            .in_progress()
            .and_then(|underway| underway.cast_due(now))
            .expect("a due caster casts");
        let team = *unit.get::<Team>().expect("a caster has a team");
        let book = world.resource::<ActionBook>();
        let owner = unit.get::<Owner>();
        let pools = unit.get_components::<UnitPoolsReadOnly>();
        let pools = pools.expect("a caster's pools").of(world.life_pool());
        let purse = Purse::of(pools, world.owner_resources(owner));
        let living = |id| view.living(id);
        let relation = |other| view.relation(team, other);
        let checked = times.checked_action(ActionCheck {
            book,
            now,
            slots,
            purse,
            aim: casting,
            relation,
            living,
        })?;
        let target = Abilities::target(view, casting.target);
        let package = checked.action.package;
        let call = CallStart {
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
            ..CallStart::cast(checked.id, checked.rank, caster.id, package, now)
        };
        let owner = owner.map(|owner| owner.slot());
        Some(Prepared {
            start: call,
            caster,
            owner,
            action: checked.id,
            rank: checked.rank,
            target,
            on_resolve: checked.action.hook(Hook::OnResolve),
            resolved: checked.resolved(ActionCall {
                aim: casting,
                start,
            }),
        })
    }

    /// Queues the prepared cast's `on_resolve` list in the frame, to the unit it aimed at, then
    /// runs its script's `on_resolve`, which queues its own effects after it.
    fn run(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        prepared: &mut Prepared,
    ) -> Result<(), CallError> {
        let call = ListCall {
            of: ListsOf::Action(prepared.action, ListHook::OnResolve),
            reached: prepared.resolved.call.aim.target,
        };
        EffectListBook::queue(batch.world(), call, ctx)?;
        let Some(script) = prepared.on_resolve else {
            return Ok(());
        };
        let target = mem::take(&mut prepared.target);
        let caster = prepared.caster.clone();
        let args = (ctx.clone(), caster, target);
        let call = HookCall {
            script,
            hook: Hook::OnResolve,
            args,
        };
        batch.call(Pool::of(prepared.owner), call).map(drop)
    }
}
