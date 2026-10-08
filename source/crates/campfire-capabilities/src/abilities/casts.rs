use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_common::Tick;
use campfire_script::ScriptId;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Keyed, Ordered, Position, SimTick, StableId, TickRate};

use crate::abilities::Abilities;
use crate::abilities::cast_spends::CastSpends;
use crate::actions::action_book::ActionBook;
use crate::actions::action_call::{ActionCall, ResolvedCast};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::effect_lists::EffectLists;
use crate::actions::in_progress::{InProgress, OrderPhase};
use crate::actions::lists_of::ListsOf;
use crate::actions::purse::Purse;
use crate::actions::targets::Targets;
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::delivering::Delivering;
use crate::items::inventory::Inventory;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::players::player_resources::PlayerResources;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::pools::Pools;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::{DashDelivery, ForcedMove};
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::values::rank::Rank;

/// A cast ready to run: the caster as the script sees it, the pool its call draws from, its
/// action and rank, its target as the script sees it, its `on_resolve`, and how it resolves. Its
/// params wait in the frame.
#[derive(Debug)]
struct Prepared {
    caster: Unit,
    pool: Pool,
    action: ActionId,
    rank: Rank,
    target: Dynamic,
    on_resolve: Option<ScriptId>,
    resolved: ResolvedCast,
}

/// The casts units are ordered: their start in Act, and in Hit their resolve, on a client
/// predicted.
#[derive(Debug)]
pub(super) struct Casts;

impl Casts {
    /// Starts each cast a unit was ordered, in Act: one that passes its checks, its target within
    /// range, starts, or for a charged action starts to charge; one whose target is beyond range,
    /// of a unit that walks, walks in range first, its walk in place of any other, and stops there
    /// to start; any other is dropped, and stops the walk it took. A unit its tags or a forced move
    /// keep from casting keeps its order, and stands: a cast it started or walked in range for goes
    /// back to it, and a charge ends, spending nothing. A charge its order released, or that is
    /// full, resolves.
    pub(super) fn start_casts(
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
            let blocked = Abilities::blocked(tags, forced, inventory, Some(kind));
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
                Self::drop_cast(&mut slots, destination.as_mut(), route, approached);
                continue;
            };
            let action = book.get(id).expect("a slot's action is in the book");
            if action.kind.kind() != ActionKind::Cast {
                continue;
            }
            if blocked {
                if approached {
                    Self::walk(destination.as_mut(), route, None);
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
                    let rank = slot.rank.expect("a slot whose toggle is on is learned");
                    slots.cool_down(aim.slot, now.after(action.values(rank).cooldown));
                }
                slots.stop();
                continue;
            }
            let purse = Purse::of(pools, resources.as_deref(), owner);
            let relation = |other| targets.relation(team, other);
            let shape = Body::shape_of(body);
            let checked = book
                .check(now, &slots, purse, aim, relation, |id| targets.living(id))
                .map(|mut checked| {
                    checked.clamp(position, shape, &targets);
                    checked
                });
            let Some(checked) = checked else {
                Self::drop_cast(&mut slots, destination.as_mut(), route, approached);
                continue;
            };
            if !checked.in_range(position, shape, &targets) {
                match (destination.is_some(), checked.aimed_at(&targets)) {
                    (true, Some(to)) => {
                        Self::walk(destination.as_mut(), route, Some(to));
                        slots.approach();
                    }
                    _ => slots.stop(),
                }
                continue;
            }
            if approached {
                Self::walk(destination.as_mut(), route, None);
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
            Self::walk(destination, route, None);
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

    /// Resolves the casts due this tick, in the order of their caster's stable id. Their calls
    /// share one snapshot of the living units, read as the batch begins, so no call sees what an
    /// earlier one changed, at once or in Resolve. A due cast whose caster's tags keep it from
    /// casting goes back to its order instead.
    pub(super) fn resolve_casts(
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
            let carried = unit.get::<Inventory>();
            let can_cast = !Abilities::blocked(unit.get::<UnitTags>(), forced, carried, Some(kind));
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
                Self::resolve(batch, &ctx, now, caster, entity);
            }
        });
    }

    /// Resolves each due cast of a unit a client predicts as the server does when the cast's script
    /// runs: one that passes its checks again cools down, and every due cast stops; one whose
    /// caster's tags keep it from casting goes back to its order. Its cost and its effects come
    /// from the server.
    pub(super) fn predict_casts(
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
            if Abilities::blocked(tags, forced, inventory, Some(kind)) {
                slots.interrupt();
                continue;
            }
            let purse = Purse::of(pools, resources.as_deref(), owner);
            let living = |id| targets.living(id);
            let relation = |other| targets.relation(team, other);
            let resolved = book
                .check(now, &slots, purse, casting, relation, living)
                .map(|checked| {
                    checked.resolved(ActionCall {
                        aim: casting,
                        start,
                    })
                });
            slots.finish_cast(now, second, resolved.as_ref());
        }
    }

    /// Resolves one cast: its script runs, then its effects, cost, cooldown and slot's changes
    /// apply together, or, when the cast no longer passes its checks or it fails, none of them, and
    /// it only stops. Of an item's action, a use of its consumable is spent last, as one used up
    /// empties its slot.
    fn resolve(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        now: Tick,
        caster: StableId,
        entity: Entity,
    ) {
        let ran = Self::prepare(batch.world(), ctx, now, caster, entity).and_then(|prepared| {
            let Some(mut prepared) = prepared else {
                return Ok(None);
            };
            Self::run(batch, ctx, &mut prepared)?;
            Self::apply(batch.world(), ctx, now, entity, &prepared);
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
        if let Some(resolved) = &resolved {
            CastSpends::spend(world, entity, resolved.call.aim.slot);
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
        if let Some(delivery) = book.get(by.action).and_then(|action| action.delivery) {
            let deliver = world.resource::<Deliverers>().of(delivery.shape);
            deliver(world, by, from, delivery, aim.target);
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
        let relation = |other| view.relation(team, other);
        let Some(checked) = book.check(now, slots, purse, casting, relation, living) else {
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
        let pool = Pool::of(owner.map(|owner| owner.slot()));
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

    /// Queues the prepared cast's `on_resolve` list in the frame, to the unit it aimed at, then
    /// runs its script's `on_resolve`, which queues its own effects after it.
    fn run(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        prepared: &mut Prepared,
    ) -> Result<(), CallError> {
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
        batch.call_hook(prepared.pool, script, Hook::OnResolve, args)
    }
}
