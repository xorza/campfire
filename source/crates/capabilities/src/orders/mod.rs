use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_script::{ScriptError, ScriptId};
use campfire_sim::{
    Command, EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, Tick, TickInputs,
    TickRate, Ticks,
};

use crate::actions::action_book::ActionBook;
use crate::actions::action_data::Range;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_slots::ActionTarget;
use crate::combat::CombatSet;
use crate::combat::dead::Dead;
use crate::combat::targets::Targets;
use crate::navigation::destination::Destination;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::orders::ai::Ai;
use crate::orders::ai_data::AiData;
use crate::orders::ai_order::AiOrder;
use crate::orders::error::AiError;
use crate::orders::next_think::NextThink;
use crate::orders::order::{Action, Order};
use crate::orders::resetting::Resetting;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::scripts::script_book::ScriptBook;
use crate::stats::StatsSet;
use crate::stats::pools::Pools;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::owner::Owner;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;

pub(crate) mod ai;
pub(crate) mod ai_data;
pub(crate) mod ai_order;
pub(crate) mod error;
pub(crate) mod next_think;
pub(crate) mod order;
pub(crate) mod orders_api;
pub(crate) mod resetting;

/// The systems of `orders`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OrdersSet {
    /// In `SimSet::Inputs`: the tick's orders become current.
    Orders,
}

/// The `orders` capability: units that take orders from a player, or from the AI script of their
/// type.
#[derive(Debug)]
pub struct Orders;

impl Orders {
    /// Adds orders to a match: in Inputs, orders become current; in Think, the resets whose units
    /// arrived end, then the units due this tick think; in Act, before combat starts attacks,
    /// units walk their paths and chase their targets. It builds on the core `Units` installs, on
    /// combat and on navigation. Without the core's scripts, as on a client, no unit thinks.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        schedule.add_systems((
            apply_orders
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders)
                .after(StatsSet::Regenerate),
            (follow_paths, chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        registry.register_component::<NextThink>();
        registry.register_component::<Resetting>();
        if !world.contains_non_send::<Ctx>() {
            return;
        }
        world.insert_resource(ByType::<Ai>::default());
        schedule.add_systems(think.in_set(SimSet::Think));
    }

    /// Gives `unit_type` its AI, with its compiled script: the think period in milliseconds
    /// becomes whole ticks at the match's rate, rounded up, and at least one.
    pub fn load_ai(
        world: &mut World,
        unit_type: UnitType,
        data: &AiData,
        script: ScriptId,
    ) -> Result<(), AiError> {
        let thinks = world
            .resource::<ScriptBook>()
            .defines(Some(script), &[Hook::OnThink])
            .contains(Hook::OnThink);
        let period = Orders::ai_period(data, *world.resource::<TickRate>(), thinks)?;
        world
            .resource_mut::<ByType<Ai>>()
            .set(unit_type, Ai { script, period });
        Ok(())
    }

    /// The think period of `data` at `rate`, a tick at the least, for a script that defines
    /// `on_think` when `thinks`: what an AI loads with, and what the package load checks at the
    /// fastest rate the mode allows, where its ticks are the most.
    pub fn ai_period(data: &AiData, rate: TickRate, thinks: bool) -> Result<Ticks, AiError> {
        let period = rate
            .ticks(data.think_ms)
            .ok_or(AiError::TimeTooLarge)?
            .max(Ticks::ONE);
        if !thinks {
            return Err(AiError::NoThink);
        }
        Ok(period)
    }

    /// Applies the next order the AI call in `frame` queued, for its unit that thinks.
    pub(crate) fn apply_next(world: &mut World, frame: &mut Frame, _: Tick) {
        let order = frame.effects.take::<AiOrder>();
        let unit = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        Orders::apply_order(world, unit, order);
    }

    /// Applies an order the AI call of `unit` queued, which the call checked against the units
    /// as the phase began; no unit dies within Think. A unit that resets takes none. A move or a
    /// reset leaves the unit's path until it is told to follow it again.
    pub(crate) fn apply_order(world: &mut World, unit: StableId, order: AiOrder) {
        let entity = world
            .resource::<EntityIndex>()
            .get(unit)
            .expect("a unit that thinks lives");
        if world.entity(entity).contains::<Resetting>() {
            return;
        }
        let target = match order {
            AiOrder::Attack { target } => Some(target),
            AiOrder::FollowPath | AiOrder::Move { .. } | AiOrder::Reset => None,
        };
        if let Some(mut slots) = world.get_mut::<ActionSlots>(entity) {
            slots.set_attack_target(target);
        }
        let to = match order {
            AiOrder::Attack { .. } => return,
            AiOrder::FollowPath => {
                if let Some(mut walker) = world.get_mut::<PathWalker>(entity) {
                    walker.rejoin();
                }
                return;
            }
            AiOrder::Move { to } => {
                let at = *world.get::<Position>(entity).expect("a unit stands");
                let to = to.get();
                world.resource::<Bounds>().ground_point([to.x, to.z], at)
            }
            AiOrder::Reset => {
                world.entity_mut(entity).insert(Resetting);
                world
                    .get::<SpawnPoint>(entity)
                    .expect("the call checked the spawn place")
                    .get()
            }
        };
        let mut unit = world.entity_mut(entity);
        if let Some(mut walker) = unit.get_mut::<PathWalker>() {
            walker.leave();
        }
        if let Some(mut destination) = unit.get_mut::<Destination>() {
            destination.set(Some(to));
        }
    }
}

/// Makes each order the current one of its unit, in input order, so a later order in the tick
/// wins. An order to a unit its player does not control, that is dead or resets, is ignored, and so
/// are a body that is not an order and an attack on a unit that is not a living enemy or that none
/// of its weapons selects: a client can send anything. A move's point clamps to the bounds, and a
/// slot's point to the ground within them, at the unit's height. A move
/// cancels an attack in its windup, and so does an attack on another target. A slot's cast or train
/// replaces an action not resolved yet, and its checks run in Act; a slot's other kind is ignored.
fn apply_orders(
    inputs: Res<'_, TickInputs>,
    bounds: Res<'_, Bounds>,
    index: Res<'_, EntityIndex>,
    book: Res<'_, ActionBook>,
    targets: Targets<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            &Owner,
            &Position,
            Option<&Team>,
            Option<&mut Destination>,
            Option<&mut ActionSlots>,
        ),
        (Without<Dead>, Without<Resetting>),
    >,
) {
    for input in inputs.iter() {
        for body in Command::bodies(input.payload, Order::CAPABILITY) {
            let Some(order) = Order::decode(body) else {
                continue;
            };
            let Some(Ok((owner, position, team, destination, slots))) =
                index.get(order.unit).map(|entity| units.get_mut(entity))
            else {
                continue;
            };
            if owner.slot() != input.slot {
                continue;
            }
            match order.action {
                Action::Move { x, z } => {
                    let Some(mut destination) = destination else {
                        continue;
                    };
                    destination.set(Some(bounds.ground_point([x, z], *position)));
                    if let Some(mut slots) = slots {
                        slots.set_attack_target(None);
                    }
                }
                Action::Attack { target } => {
                    let selected = team.and_then(|&team| {
                        let unit = targets.enemy(team, target)?;
                        Some((targets.attitude(team, unit.team), unit.tags))
                    });
                    if let (Some(selected), Some(mut slots)) = (selected, slots)
                        && book.weapon_for(&slots, Some(selected)).is_some()
                    {
                        slots.set_attack_target(Some(target));
                    }
                }
                Action::Slot { slot, target } => {
                    let Some(mut slots) = slots else {
                        continue;
                    };
                    let kind = slots
                        .slot(slot)
                        .and_then(|held| book.get(held.action))
                        .map(|action| action.kind);
                    let target = match target {
                        ActionTarget::Point(at) => {
                            let at = at.get();
                            ActionTarget::Point(bounds.ground_point([at.x, at.z], *position))
                        }
                        target => target,
                    };
                    if let Some(kind @ (ActionKind::Cast | ActionKind::Train)) = kind {
                        slots.order(slot, kind, target);
                    }
                }
            }
        }
    }
}

/// A unit due to think this tick: since which tick, and with which script and period.
#[derive(Debug, Clone, Copy)]
struct Due {
    since: Tick,
    id: StableId,
    entity: Entity,
    script: ScriptId,
    period: Ticks,
}

/// A reset that ends this tick: its unit, and whether it died.
#[derive(Debug, Clone, Copy)]
struct Ended {
    entity: Entity,
    dead: bool,
}

/// Runs `on_think` for each living unit of a type with AI that is due, those due longest first,
/// then by stable id. A unit is first due in the first tick that leaves the remainder of its
/// stable id when divided by its type's period, so the units of a type spread over the period;
/// then a period after each think. Each call's orders apply when it returns; a failed call's do
/// not. A unit whose call finds the think pool spent stays due, so under load AI thinks later,
/// and no unit misses its turn for good. First, each reset whose unit arrived, its destination
/// dropped, ends with its pools full, and each whose unit died ends with nothing more: its AI
/// thinks free of it.
fn think(
    world: &mut World,
    resetting: &mut QueryState<(Entity, Has<Dead>, Option<&Destination>), With<Resetting>>,
    thinkers: &mut QueryState<(Entity, &StableId, &UnitType, Option<&NextThink>), Without<Dead>>,
    (mut due, mut reset): (Local<'_, Vec<Due>>, Local<'_, Vec<Ended>>),
) {
    let now = world.resource::<SimTick>().start();
    reset.clear();
    reset.extend(
        resetting
            .iter(world)
            .filter(|(_, dead, destination)| {
                *dead || destination.is_none_or(|destination| destination.get().is_none())
            })
            .map(|(entity, dead, _)| Ended { entity, dead }),
    );
    due.clear();
    let book = world.resource::<ByType<Ai>>();
    for (entity, &id, &unit_type, next) in thinkers.iter(world) {
        let Some(ai) = book.get(unit_type) else {
            continue;
        };
        let period = ai.period;
        let since = match next {
            Some(next) => next.get(),
            None if now.get() % period.get() == id.get() % period.get() => now,
            None => continue,
        };
        if since <= now {
            due.push(Due {
                since,
                id,
                entity,
                script: ai.script,
                period,
            });
        }
    }
    for &Ended { entity, dead } in &*reset {
        let mut unit = world.entity_mut(entity);
        unit.remove::<Resetting>();
        if let (false, Some(mut pools)) = (dead, unit.get_mut::<Pools>()) {
            pools.fill();
        }
    }
    if due.is_empty() {
        return;
    }
    due.sort_unstable_by_key(|due| (due.since, due.id));
    let ctx = world.non_send::<Ctx>().clone();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &Due {
            since,
            id,
            entity,
            script,
            period,
        } in &*due
        {
            // A unit with no position or team is no unit scripts see.
            let Some(unit) = ctx.view().unit(id) else {
                continue;
            };
            ctx.frame().begin_think(batch.world(), id);
            let next = match batch.call(Pool::Think, script, Hook::OnThink, (ctx.clone(), unit)) {
                Ok(_) => {
                    ctx.apply(batch.world(), now);
                    now.after(period)
                }
                Err(ScriptError::TickBudget) => since,
                Err(error) => {
                    batch.record(Some(id), Hook::OnThink, CallError::from_script(error));
                    now.after(period)
                }
            };
            batch
                .world()
                .entity_mut(entity)
                .insert(NextThink::new(next));
        }
    });
}

/// Sends each path walker with no attack target, on its path, to the waypoint it walks to, and on to the next
/// once the waypoint is within its body, or it stands on the waypoint with no body: walkers that
/// push each other never stand on one point. A walker that chased a target walks back to where it
/// left its path.
fn follow_paths(
    paths: Res<'_, Paths>,
    mut walkers: Query<
        '_,
        '_,
        (
            &Position,
            &OnPath,
            &mut PathWalker,
            Option<&ActionSlots>,
            &mut Destination,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, path, mut walker, slots, mut destination, body) in &mut walkers {
        if walker.left() || slots.is_some_and(|slots| slots.attack_target().is_some()) {
            continue;
        }
        let path = path.get();
        let mut waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        if waypoint.is_some_and(|at| position.within_ground(at, Body::radius_of(body))) {
            walker.advance();
            waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        }
        walk_to(&mut destination, waypoint);
    }
}

/// Walks each unit that can move to its attack target while out of the range of the weapon it
/// attacks it with, and stops it in range or in its windup. A unit whose target is gone, dead, no
/// longer an enemy or one no weapon of it selects drops it and stops.
fn chase(
    book: Res<'_, ActionBook>,
    targets: Targets<'_, '_>,
    mut chasers: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &mut ActionSlots,
            &mut Destination,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, &team, mut slots, mut destination, body) in &mut chasers {
        let Some(target) = slots.attack_target() else {
            continue;
        };
        if slots.attacking().is_some() {
            continue;
        }
        let aimed = targets.enemy(team, target).and_then(|unit| {
            let selected = (targets.attitude(team, unit.team), unit.tags);
            let slot = book.weapon_for(&slots, Some(selected))?;
            Some((unit, book.range(&slots, slot)))
        });
        match aimed {
            None => {
                slots.set_attack_target(None);
                walk_to(&mut destination, None);
            }
            Some((unit, Range::Meters(range)))
                if !targets.reaches(position, Body::radius_of(body), range, &unit) =>
            {
                walk_to(&mut destination, Some(unit.pos));
            }
            Some(_) => walk_to(&mut destination, None),
        }
    }
}

/// Sets where a unit walks, leaving a destination that does not change untouched: a write marks
/// it changed, and an avatar's destination replicates.
fn walk_to(destination: &mut Mut<'_, Destination>, target: Option<Position>) {
    if destination.get() != target {
        destination.set(target);
    }
}

#[cfg(test)]
mod tests;
