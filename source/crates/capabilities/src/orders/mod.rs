use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_math::Vec3;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{
    Command, EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, Tick, TickInputs,
    TickRate, Ticks,
};

use crate::abilities::ability_slots::AbilitySlots;
use crate::combat::CombatSet;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::targets::Targets;
use crate::navigation::destination::Destination;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::orders::ai::Ai;
use crate::orders::ai_ctx::{AiCtx, AiOrder};
use crate::orders::ai_data::AiData;
use crate::orders::error::AiError;
use crate::orders::next_think::NextThink;
use crate::orders::order::{Action, Order};
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::owner::Owner;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;

pub(crate) mod ai;
pub(crate) mod ai_ctx;
pub(crate) mod ai_data;
pub(crate) mod error;
pub(crate) mod next_think;
pub(crate) mod order;

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
    /// Adds orders to a match: in Inputs, orders become current; in Think, the units due this
    /// tick think; in Act, before combat starts attacks, units walk their paths and chase their
    /// targets. It builds on the core `Units` installs, on combat and on navigation. Without the
    /// core's scripts, as on a client, no unit thinks.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        schedule.add_systems((
            apply_orders
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders),
            (follow_paths, chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        registry.register_component::<NextThink>();
        let Some(mut host) = world.get_non_send_mut::<ScriptHost>() else {
            return;
        };
        AiCtx::register(host.engine_mut());
        let ctx = AiCtx::new(world.non_send::<View>().clone());
        world.insert_non_send(ctx);
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
        let period = world
            .resource::<TickRate>()
            .ticks(data.think_ms)
            .ok_or(AiError::TimeTooLarge)?
            .max(Ticks::ONE);
        let host = world.non_send::<ScriptHost>();
        if !host.defines(script, Hook::Think.name(), Hook::Think.params()) {
            return Err(AiError::NoThink);
        }
        world
            .resource_mut::<ByType<Ai>>()
            .set(unit_type, Ai { script, period });
        Ok(())
    }
}

/// Makes each order the current one of its unit, in input order, so a later order in the tick
/// wins. An order to a unit its player does not control, or that is dead, is ignored, and so are
/// a body that is not an order and an attack on a unit that is not a living enemy: a client can
/// send anything. A move's point clamps to the bounds. A move cancels an attack in its windup, and
/// so does an attack on another target. A cast replaces a cast not resolved yet; its checks run
/// in Act.
fn apply_orders(
    inputs: Res<'_, TickInputs>,
    bounds: Res<'_, Bounds>,
    index: Res<'_, EntityIndex>,
    targets: Targets<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            &Owner,
            &Position,
            Option<&Team>,
            Option<&mut Destination>,
            Option<&mut AttackState>,
            Option<&mut AbilitySlots>,
        ),
        Without<Dead>,
    >,
) {
    for input in inputs.iter() {
        for body in Command::bodies(input.payload, Order::CAPABILITY) {
            let Some(order) = Order::decode(body) else {
                continue;
            };
            let Some(Ok((owner, position, team, destination, attack, slots))) =
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
                    // Orders name a point on the ground plane; the unit keeps its height.
                    let [x, z] = bounds.clamp_ground([x, z]);
                    let target = Position::new(Vec3::new(x, position.get().y, z))
                        .expect("bounds are within the world's bound");
                    destination.set(Some(target));
                    if let Some(mut attack) = attack {
                        attack.set_target(None);
                    }
                }
                Action::Attack { target } => {
                    let enemy = team.is_some_and(|&team| targets.enemy(team, target).is_some());
                    if let (true, Some(mut attack)) = (enemy, attack) {
                        attack.set_target(Some(target));
                    }
                }
                Action::Cast { slot, target } => {
                    if let Some(mut slots) = slots {
                        slots.order(slot, target);
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

/// Runs `think` for each living unit of a type with AI that is due, those due longest first,
/// then by stable id. A unit is first due in the first tick that leaves the remainder of its
/// stable id when divided by its type's period, so the units of a type spread over the period;
/// then a period after each think. Each call's orders apply when it returns; a failed call's do
/// not. A unit whose call finds the think pool spent stays due, so under load AI thinks later,
/// and no unit misses its turn for good.
fn think(world: &mut World, mut due: Local<'_, Vec<Due>>) {
    let now = world.resource::<SimTick>().start();
    due.clear();
    let book = world.resource::<ByType<Ai>>();
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        let unit = world.entity(entity);
        let ai = unit
            .get::<UnitType>()
            .and_then(|&unit_type| book.get(unit_type));
        let Some(ai) = ai.filter(|_| !unit.contains::<Dead>()) else {
            continue;
        };
        let period = ai.period;
        let since = match unit.get::<NextThink>() {
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
    if due.is_empty() {
        return;
    }
    due.sort_unstable_by_key(|due| (due.since, due.id));
    let ctx = world.non_send::<AiCtx>().clone();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &Due {
            since,
            id,
            entity,
            script,
            period,
        } in &*due
        {
            // A unit with no team or health is no unit scripts see.
            let Some(unit) = ctx.view().unit(id) else {
                continue;
            };
            ctx.begin(id);
            ctx.view().set_caller(0);
            let next = match batch.call(Pool::Think, script, Hook::Think, (ctx.clone(), unit)) {
                Ok(_) => {
                    for order in ctx.frame().orders.drain(..) {
                        apply_ai_order(batch.world(), id, order);
                    }
                    now.after(period)
                }
                Err(ScriptError::TickBudget) => since,
                Err(error) => {
                    batch.record(Some(id), Hook::Think, CallError::from_script(error));
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

/// Applies an order the AI call of `unit` queued, which the call checked against the units as
/// the phase began; no unit dies within Think.
fn apply_ai_order(world: &mut World, unit: StableId, order: AiOrder) {
    let target = match order {
        AiOrder::Attack { target } => Some(target),
        AiOrder::FollowPath => None,
    };
    let entity = world
        .resource::<EntityIndex>()
        .get(unit)
        .expect("a unit that thinks lives");
    if let Some(mut attack) = world.get_mut::<AttackState>(entity) {
        attack.set_target(target);
    }
}

/// Sends each path walker with no attack target to the waypoint it walks to, and on to the next
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
            &AttackState,
            &mut Destination,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, path, mut walker, attack, mut destination, body) in &mut walkers {
        if attack.target().is_some() {
            continue;
        }
        let path = path.get();
        let mut waypoint = paths.waypoint(path, walker.next(), walker.direction());
        if waypoint.is_some_and(|at| position.within_ground(at, Body::radius_of(body))) {
            walker.advance();
            waypoint = paths.waypoint(path, walker.next(), walker.direction());
        }
        walk_to(&mut destination, waypoint);
    }
}

/// Walks each unit that can move to its attack target while out of range, and stops it in range
/// or in its windup. A unit whose target is gone, dead or no longer an enemy drops it and stops.
fn chase(
    targets: Targets<'_, '_>,
    mut chasers: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &AttackStats,
            &mut AttackState,
            &mut Destination,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, &team, stats, mut attack, mut destination, body) in &mut chasers {
        let Some(target) = attack.target() else {
            continue;
        };
        if attack.started().is_some() {
            continue;
        }
        match targets.enemy(team, target) {
            None => {
                attack.set_target(None);
                walk_to(&mut destination, None);
            }
            Some(unit) if stats.reaches(position, Body::radius_of(body), &unit) => {
                walk_to(&mut destination, None);
            }
            Some(unit) => walk_to(&mut destination, Some(unit.pos)),
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
