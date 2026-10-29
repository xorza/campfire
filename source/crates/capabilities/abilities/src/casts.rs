//! The casts units are ordered: their start in Act, and in Hit their resolve, on a client
//! predicted.

use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::query::{Has, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_actions::{
    ActionBook, ActionCall, ActionCheck, ActionKind, ActionProgress, ActionSlots, CastSpends,
    Caster, CheckedAction, EffectListBook, InProgress, ListCall, ListHook, ListsOf, OnResolve,
    OrderPhase, PlayerCooldowns, ProgressDues, ProgressIndexes, Purse, RangeFit, ResolveKind,
    ResolvedCast, SlotCommit, SlotTimes, StartChecked, StartFrom, Targets, UseGroup,
};
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_deliveries::{Deliverers, Delivering, Launcher};
use campfire_math::{Num, RngStream, Rounding, Vec3};
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut, PlayersCall, PlayersWorld};
use campfire_script::{
    ActionCast, Answered, CallError, CallStart, Ctx, ScriptBatch, ScriptId, ScriptValue,
    ScriptsWorld, Unit,
};
use campfire_sim::{
    DueUnit, Keyed, MatchRate, Ordered, Position, SimRng, SimTick, StableId, TickClock,
};
use campfire_stats::{PoolBook, StatsWorld, UnitPools, UnitPoolsReadOnly};
use campfire_units::{Body, Dead, Due, ForcedMove, Life, Owner, PoolPayer, Walk, WalkItem};
use campfire_values::{ActionId, Allegiance, DashDelivery, Motion, Rank, Team, UnitTags};

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

/// A caster whose cast is due, keyed by its stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CastDue(Keyed);

/// What the collect of the due casts reads of a caster, and its progress, which a cast it may not
/// resolve goes back to its order in.
#[derive(Debug, QueryData)]
#[query_data(mutable, derive(Debug))]
pub(crate) struct DueCaster {
    entity: Entity,
    id: &'static StableId,
    progress: &'static mut ActionProgress,
    slots: &'static ActionSlots,
    tags: Option<&'static UnitTags>,
    forced: Has<ForcedMove>,
    used: Option<&'static UseGroup>,
    life: Option<&'static Life>,
}

/// The stream a cast's spread windup draws from, once for each unit that starts a cast in a tick.
const WINDUP_STREAM: RngStream = RngStream::new("abilities.windup");

#[derive(Debug)]
pub struct Casts;

impl Casts {
    /// Starts each cast or use a unit was ordered, of the units the index of the orders holds, in
    /// Act: one that passes its checks, its target
    /// within range, starts, or for a charged action starts to charge; one whose target is beyond
    /// range, of a unit that walks, walks in range first, its walk in place of any other, and stops
    /// there to start; any other is dropped, and stops the walk it took. A unit its tags or a
    /// forced move keep from casting, or that lives no more, keeps its order, and stands: a cast
    /// it started or walked in range for goes back to it, and a charge ends, spending nothing. A
    /// charge its order released, or that is full, resolves.
    pub(super) fn start_casts(
        (tick, rng): (Res<'_, SimTick>, Res<'_, SimRng>),
        book: Res<'_, ActionBook>,
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        cooldowns: PlayerRows<'_, '_, PlayerCooldowns>,
        targets: Targets<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                (Entity, &StableId, &Position),
                &Team,
                (&ActionSlots, &mut ActionProgress),
                &mut SlotTimes,
                UnitPoolsReadOnly,
                Option<&Owner>,
                Option<&Body>,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<Walk>,
                Option<&UseGroup>,
            ),
            Without<Dead>,
        >,
        (mut indexes, mut held): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
    ) {
        let now = tick.start();
        let life = life.life();
        held.clear();
        indexes.orders(&mut held);
        for &DueUnit { id, entity } in &*held {
            // A dead unit keeps its order, for when it lives again; a gone one left the index as
            // its progress went.
            let Ok(unit) = units.get_mut(entity) else {
                continue;
            };
            let (
                (_, _, &position),
                &team,
                (slots, mut progress),
                mut times,
                pools,
                owner,
                body,
                tags,
                forced,
                mut walk,
                used,
            ) = unit;
            let Some(underway) = progress.in_progress().filter(|_| progress.has_order()) else {
                indexes.forget_order(entity);
                continue;
            };
            let dues = &mut indexes.of(id, entity);
            let underway_slot = slots.slot(underway.slot()).expect("a slot the unit has");
            let blocked = Caster::of(tags, Motion::of(forced), used)
                .blocked(Some(underway_slot), &book)
                || !pools.lives();
            if let InProgress::Charge { .. } = underway {
                if blocked {
                    progress.interrupt(dues);
                } else {
                    progress.release(now, dues);
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
            let Some(action) = slot.action else {
                Self::drop_cast(&mut progress, walk.as_mut(), approached, dues);
                continue;
            };
            let action = book.get(action).expect("a slot's action is in the book");
            if !matches!(
                action.kind,
                ActionKind::Cast | ActionKind::Use | ActionKind::Enter
            ) {
                continue;
            }
            if blocked {
                if approached {
                    Self::walk(walk.as_mut(), None);
                }
                progress.interrupt(dues);
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
                progress.end(dues);
                continue;
            }
            let purse = Purse::of(pools.of(life), resources.of(owner));
            let relation = |other| targets.relation(team, other);
            let from = StartFrom {
                position: position.get(),
                body,
                targets: &targets,
            };
            let check = ActionCheck {
                book: &book,
                now,
                caster: Caster::of(tags, Motion::of(forced), used),
                slots,
                player: cooldowns.of(owner),
                purse,
                aim,
                relation,
                living: |id| targets.target(id),
            };
            let Some(StartChecked { checked, reach }) = times.start(check, Some(from)) else {
                Self::drop_cast(&mut progress, walk.as_mut(), approached, dues);
                continue;
            };
            if reach != RangeFit::Within {
                let to = checked.walk_to(position.get(), body, reach, &targets);
                // A walk that arrived short of its goal ends its cast, as no path comes nearer
                // (riders-and-upgrades.md, An approach that cannot reach ends).
                match (walk.as_mut(), to) {
                    (Some(walk), Some(to)) if !walk.gives_up(to) => {
                        walk.to(Some(to));
                        progress.approach();
                    }
                    _ => progress.end(dues),
                }
                continue;
            }
            if approached {
                Self::walk(walk.as_mut(), None);
            }
            let windup = Self::windup(&checked, &rng, id);
            progress.begin(&checked, position.get(), now, windup, dues);
        }
    }

    /// The windup of the cast `checked` of the unit `id`: its action's, spread each way by its
    /// channel's `windup_spread`, a factor drawn even in that spread from the unit's stream of
    /// windups in the running tick, to whole ticks down, as Zero Hour draws and truncates a
    /// hacker's unpack (04-capabilities/actions.md, A channel with no end). A predicting client,
    /// which holds no seed, draws another, and takes the server's start as it comes.
    fn windup(checked: &CheckedAction<'_>, rng: &SimRng, id: StableId) -> Ticks {
        let windup = checked.values.windup;
        let spread = checked.values.channel.and_then(|rule| rule.windup_spread);
        let Some(spread) = spread.filter(|&spread| spread > Num::ZERO) else {
            return windup;
        };
        let draw = rng.open(WINDUP_STREAM, id).fraction();
        let factor = (spread + spread)
            .checked_mul(draw, Rounding::Floor)
            .and_then(|share| (Num::ONE - spread).checked_add(share))
            .expect("a factor below 2");
        let ticks = Num::int(i64::try_from(windup.get()).expect("a windup fits i64"))
            .checked_mul(factor, Rounding::Floor)
            .expect("a windup below twice its ticks");
        Ticks::new(u64::try_from(ticks.floor()).expect("a spread windup is not negative"))
    }

    /// Drops the cast ordered in `progress`, and stops the walk it took when it `approached`.
    fn drop_cast(
        progress: &mut ActionProgress,
        walk: Option<&mut WalkItem<'_, '_>>,
        approached: bool,
        dues: &mut ProgressDues<'_>,
    ) {
        if approached {
            Self::walk(walk, None);
        }
        progress.end(dues);
    }

    /// Walks the unit of `walk`, one that walks, to `to`, or stops it.
    fn walk(walk: Option<&mut WalkItem<'_, '_>>, to: Option<Vec3>) {
        if let Some(walk) = walk {
            walk.to(to);
        }
    }

    /// Collects the casts due this tick, in the order of their caster's stable id, for
    /// `resolve_casts`, from the index of the resolves. A due cast whose caster's tags keep it
    /// from casting, or that lives no more, goes back to its order instead.
    pub(super) fn collect_casts(
        (tick, book): (Res<'_, SimTick>, Res<'_, ActionBook>),
        mut casters: Query<'_, '_, DueCaster, Without<Dead>>,
        mut indexes: ProgressIndexes<'_>,
        (mut order, mut taken): (Local<'_, Ordered>, Local<'_, Vec<DueUnit>>),
        mut due: ResMut<'_, Due<CastDue>>,
    ) {
        let now = tick.start();
        taken.clear();
        let progress = |entity| casters.get(entity).ok().map(|caster| caster.progress);
        indexes.take_resolves(now, ResolveKind::Cast, progress, &mut taken);
        let resolving = taken
            .iter()
            .map(|&DueUnit { id, entity }| Keyed { id, entity });
        let sorted = order.sort(resolving);
        due.refill(sorted.iter().map(|&keyed| CastDue(keyed)));
        due.retain(|&CastDue(Keyed { id, entity })| {
            let mut caster = casters.get_mut(entity).expect("a due caster");
            let underway = caster
                .progress
                .in_progress()
                .expect("a due cast is under way");
            let slot = caster.slots.slot(underway.slot()).expect("a cast's slot");
            let motion = Motion::of(caster.forced);
            let can_cast = campfire_units::lives(false, caster.life)
                && !Caster::of(caster.tags, motion, caster.used).blocked(Some(slot), &book);
            if !can_cast {
                caster.progress.interrupt(&mut indexes.of(id, entity));
            }
            can_cast
        });
    }

    /// Resolves the casts `collect_casts` found due, in its order. Their calls share one snapshot
    /// of the living units, read as the batch begins, so no call sees what an earlier one
    /// changed, at once or in Resolve.
    pub(super) fn resolve_casts(world: &mut World) {
        let due = world.resource_mut::<Due<CastDue>>().take();
        if !due.is_empty() {
            let now = world.resource::<SimTick>().start();
            world.script_batch(|batch| {
                for &CastDue(Keyed { id: caster, entity }) in &due {
                    Self::resolve(batch, now, caster, entity);
                }
            });
        }
        world.resource_mut::<Due<CastDue>>().give_back(due);
    }

    /// Resolves each due cast of a unit a client predicts as the server does when the cast's script
    /// runs: one that passes its checks again cools down, and every due cast stops; one whose
    /// caster's tags keep it from casting, or that lives no more, goes back to its order. Its
    /// cost and its effects come from the server.
    pub(super) fn predict_casts(
        clock: TickClock<'_>,
        book: Res<'_, ActionBook>,
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        mut cooldowns: PlayerRowsMut<'_, '_, PlayerCooldowns>,
        targets: Targets<'_, '_>,
        mut casters: Query<
            '_,
            '_,
            (
                (Entity, &StableId, &Team),
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
        (mut indexes, mut taken): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
    ) {
        let now = clock.start();
        let life = life.life();
        let second = Abilities::second(clock.rate());
        taken.clear();
        let progress = |entity| {
            casters
                .get(entity)
                .ok()
                .map(|(_, (_, progress), ..)| progress)
        };
        indexes.take_resolves(now, ResolveKind::Cast, progress, &mut taken);
        for &DueUnit { id, entity } in &*taken {
            let ((_, _, &team), (slots, mut progress), mut times, pools, owner, tags, forced, used) =
                casters.get_mut(entity).expect("a due caster");
            let dues = &mut indexes.of(id, entity);
            let ActionCall {
                aim: casting,
                start,
            } = progress
                .in_progress()
                .and_then(|underway| underway.cast_due(now))
                .expect("a due cast is under way");
            let slot = slots.slot(casting.slot).expect("a cast's slot");
            if Caster::of(tags, Motion::of(forced), used).blocked(Some(slot), &book)
                || !pools.lives()
            {
                progress.interrupt(dues);
                continue;
            }
            let purse = Purse::of(pools.of(life), resources.of(owner));
            let living = |id| targets.target(id);
            let relation = |other| targets.relation(team, other);
            let check = ActionCheck {
                book: &book,
                now,
                caster: Caster::of(tags, Motion::of(forced), used),
                slots,
                player: cooldowns.of(owner),
                purse,
                aim: casting,
                relation,
                living,
            };
            let resolved = times.start(check, None).map(|started| {
                started.checked.resolved(ActionCall {
                    aim: casting,
                    start,
                })
            });
            progress.complete_cast(slots, now, resolved.as_ref(), dues);
            if let Some(resolved) = &resolved {
                let player = cooldowns.of_mut(owner);
                times.commit(
                    SlotCommit::cast(resolved, now, second),
                    player.map(Mut::into_inner),
                );
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
            let hook = ListHook::OnResolve.hook();
            let ran = batch.answer(prepared.start, hook, Some(caster), |batch| {
                Self::pay_resources(batch.world(), ctx, &prepared);
                Self::run(batch, ctx, &mut prepared)?;
                Self::launch(batch.world(), entity, &prepared);
                Ok(())
            });
            match ran {
                Answered::Ran(()) => {
                    Self::pay_pools(batch.world(), entity, &prepared);
                    Some(prepared.resolved)
                }
                Answered::Failed | Answered::Waits => None,
            }
        });
        let world = batch.world();
        let second = Abilities::second(*world.resource::<MatchRate>());
        ProgressDues::with(world, caster, entity, |world, dues| {
            let [mut unit] = world.entity_mut([entity]);
            let (slots, mut progress) = unit
                .get_components_mut::<(&ActionSlots, &mut ActionProgress)>()
                .expect("a caster has slots");
            progress.complete_cast(slots, now, resolved.as_ref(), dues);
        });
        if let Some(resolved) = &resolved {
            let owner = world.get::<Owner>(entity).map(|owner| owner.player_slot());
            let player = owner.and_then(|owner| world.player_entity(owner));
            let player = player.filter(|&player| world.get::<PlayerCooldowns>(player).is_some());
            let commit = SlotCommit::cast(resolved, now, second);
            match player {
                Some(player) => {
                    let [mut unit, mut player] = world.entity_mut([entity, player]);
                    let mut times = unit
                        .get_mut::<SlotTimes>()
                        .expect("a caster has slot times");
                    let held = player.get_mut::<PlayerCooldowns>();
                    times.commit(commit, held.map(Mut::into_inner));
                }
                None => world
                    .get_mut::<SlotTimes>(entity)
                    .expect("a caster has slot times")
                    .commit(commit, None),
            }
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
            .extension_mut::<PlayersCall>()
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
            let deliver = world.resource::<Deliverers>().of(delivery.capability);
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
        let check = ActionCheck {
            book,
            now,
            caster: Caster::of(
                unit.get::<UnitTags>(),
                Motion::of(unit.contains::<ForcedMove>()),
                unit.get::<UseGroup>(),
            ),
            slots,
            player: owner
                .and_then(|owner| world.player_entity(owner.player_slot()))
                .and_then(|player| world.get::<PlayerCooldowns>(player)),
            purse,
            aim: casting,
            relation,
            living,
        };
        let checked = times.start(check, None)?.checked;
        let target = Abilities::target(view, casting.target);
        let package = checked.action.package;
        let owner = owner.map(|owner| owner.player_slot());
        let allegiance = Allegiance { team, owner };
        let cast = ActionCast {
            action: checked.id,
            rank: checked.rank,
            caster: caster.id,
            allegiance,
            package,
        };
        let call = CallStart {
            start: Some(start),
            dash_delivers: checked.action.delivery.is_none().then_some(DashDelivery {
                source: caster.id.into(),
                action: checked.id,
                rank: checked.rank,
                start: Some(start),
            }),
            payer: owner,
            ..CallStart::cast(cast, now)
        };
        Some(Prepared {
            start: call,
            caster,
            owner,
            action: checked.id,
            rank: checked.rank,
            target,
            on_resolve: checked.action.hook(ListHook::OnResolve.hook()),
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
        batch.call::<OnResolve>(script, args).map(drop)
    }
}
