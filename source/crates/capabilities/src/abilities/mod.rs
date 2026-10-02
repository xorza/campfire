use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_math::{Tick, Ticks};
use campfire_script::ScriptId;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::abilities::effect_lists::EffectLists;

use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::delivery::{Delivery, DeliveryShape};
use crate::scripts::call_start::CallStart;
use crate::units::action_id::ActionId;

use crate::actions::action_slots::{ActionSlots, InProgress};

use crate::actions::action_target::ActionTarget;
use crate::actions::purse::{Payer, Purse};
use crate::areas::Areas;
use crate::combat::CombatSet;

use crate::actions::targets::Targets;
use crate::deliveries::delivering::Delivering;
use crate::players::player_resources::PlayerResources;
use crate::units::body::Body;
use crate::units::dead::Dead;

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
    /// script host; without the core's scripts, as on a client, a due cast of a unit it predicts
    /// only cools down, as the server's does.
    pub fn install(world: &mut World, schedule: &mut Schedule, _: &mut StateRegistry) {
        schedule.add_systems(start_casts.in_set(SimSet::Act).in_set(ActionsSet::Start));
        if !world.contains_non_send::<Ctx>() {
            schedule.add_systems(
                predict_casts
                    .in_set(SimSet::Hit)
                    .after(CombatSet::Fire)
                    .before(CombatSet::Launch),
            );
            return;
        }
        world.insert_resource(EffectLists::default());
        schedule.add_systems(
            resolve_casts
                .in_set(SimSet::Hit)
                .after(CombatSet::Fire)
                .before(CombatSet::Launch),
        );
    }
}

/// Starts each cast a unit was ordered, in Act: one that passes its checks, its target within
/// range, starts, and any other is dropped. A unit its tags keep from casting keeps its order: a
/// cast it started goes back to it.
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
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, mut slots, pools, owner, body, tags) in &mut units {
        let Some(InProgress::Order { aim, resolves_at }) = slots.in_progress() else {
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
        if UnitTags::effects_of(tags).blocks(Block::Cast) {
            if resolves_at.is_some() {
                slots.interrupt();
            }
            continue;
        }
        if resolves_at.is_some() {
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
            .filter(|checked| checked.in_range(position, radius, &targets))
            .map(|checked| (now.after(checked.values.windup), checked.target));
        match started {
            Some((resolves_at, target)) => slots.start(resolves_at, target),
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

/// Resolves each due cast of a unit a client predicts as the server does when the cast's script
/// runs: one that passes its checks again cools down, and every due cast stops; one whose caster's
/// tags keep it from casting goes back to its order. Its cost and its effects come from the
/// server.
fn predict_casts(
    tick: Res<'_, SimTick>,
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
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&team, mut slots, pools, owner, tags) in &mut casters {
        let Some(casting) = slots
            .in_progress()
            .and_then(|underway| underway.cast_due(now))
        else {
            continue;
        };
        if UnitTags::effects_of(tags).blocks(Block::Cast) {
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
        let cooldown = book
            .check(now, &slots, purse, casting, attitude, living)
            .map(|checked| checked.values.cooldown);
        if let Some(cooldown) = cooldown {
            slots.cool_down(casting.slot, now.after(cooldown));
        }
        slots.stop();
    }
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
    let casting = slots
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
    frame.begin(
        world,
        CallStart::cast(checked.id, checked.rank, caster.id, package),
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

#[cfg(test)]
pub(crate) mod internals {
    use crate::abilities::Abilities;
    use crate::abilities::effect_lists::{EffectLists, Listed};
    use crate::abilities::effect_names::EffectNames;
    use crate::actions::action_data::ActionData;
    use crate::progression::tracks_column::TracksColumn;
    use crate::stats::Stats;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::script_view::View;
    use crate::units::track_id::TrackId;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;

    impl Abilities {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        pub(crate) fn load_effects(
            world: &mut World,
            action: ActionId,
            package: u16,
            data: &ActionData,
        ) {
            let view = world.non_send::<View>().clone();
            let names = MatchEffectNames {
                world,
                view: &view,
                action,
                package,
            };
            let lists = Listed::lists_of(data, &names);
            world.resource_mut::<EffectLists>().push(action, lists);
        }
    }

    /// The names of an action's effect lists as a match's world resolves them: its view, its param
    /// book, and the modifiers of the action's package.
    #[derive(Debug)]
    struct MatchEffectNames<'w> {
        world: &'w World,
        view: &'w View,
        action: ActionId,
        package: u16,
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> usize {
            self.world
                .resource::<ParamBook>()
                .actions()
                .named(self.action.index(), name.as_str())
                .expect("the load checked an effect's param")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind_named(name.as_str())
                .expect("the load checked an effect's damage kind")
        }

        fn pool(&self, name: &DeclaredName) -> PoolId {
            StatsColumn::pool_id_named(self.view, name.as_str())
                .expect("the load checked an effect's pool")
        }

        fn modifier(&self, name: &DeclaredName) -> ModifierId {
            Stats::modifier(self.world, self.package, name.as_str())
                .expect("the load checked an effect's modifier")
        }

        fn track(&self, name: &DeclaredName) -> TrackId {
            TracksColumn::track_named(self.view, name.as_str())
                .expect("the load checked an effect's track")
        }
    }
}

#[cfg(test)]
mod tests;
