use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_math::Vec3;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{
    Command, EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, TickInputs, TickRate,
};

use crate::abilities::ability_slots::AbilitySlots;
use crate::combat::CombatSet;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::targets::Targets;
use crate::combat::team::Team;
use crate::control::ai_book::{Ai, AiBook};
use crate::control::ai_ctx::{AiCtx, AiOrder};
use crate::control::ai_data::AiData;
use crate::control::controller::Controller;
use crate::control::error::AiError;
use crate::control::next_think::NextThink;
use crate::control::order::{Action, Order};
use crate::navigation::destination::Destination;
use crate::navigation::lane_walker::LaneWalker;
use crate::navigation::lanes::Lanes;
use crate::units::error::CallError;
use crate::units::script_budgets::ScriptBudgets;
use crate::units::script_failures::{Hook, ScriptFailure, ScriptFailures};
use crate::units::script_view::View;
use crate::units::unit_type::UnitType;

pub(crate) mod ai_book;
pub(crate) mod ai_ctx;
pub(crate) mod ai_data;
pub(crate) mod controller;
pub(crate) mod error;
pub(crate) mod next_think;
pub(crate) mod order;

/// The `control` capability: who moves a unit. For now, of the orders kind: units that take
/// orders from a player, or from the AI script of their type.
#[derive(Debug)]
pub struct Control;

impl Control {
    /// Adds control to a match: in Inputs, orders become current; in Think, the units due this
    /// tick think; in Act, before combat starts attacks, units walk their paths and chase their
    /// targets. It builds on the core `Units` installs, on combat and on navigation.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        AiCtx::register(
            world
                .get_non_send_mut::<ScriptHost>()
                .expect("Units::install runs first")
                .into_inner()
                .engine_mut(),
        );
        let ctx = AiCtx::new(world.non_send::<View>().clone());
        world.insert_non_send(ctx);
        world.insert_resource(AiBook::default());
        schedule.add_systems((
            apply_orders.in_set(SimSet::Inputs),
            think.in_set(SimSet::Think),
            (follow_paths, chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        registry.register_component::<Controller>();
        registry.register_component::<NextThink>();
    }

    /// Gives `unit_type` its AI, with the source of its script: the think period in milliseconds
    /// becomes whole ticks at the match's rate, rounded up, and at least one.
    pub fn load_ai(
        world: &mut World,
        unit_type: UnitType,
        data: &AiData,
        source: &str,
    ) -> Result<(), AiError> {
        let period = world
            .resource::<TickRate>()
            .ticks(data.think_ms)
            .and_then(|ticks| u32::try_from(ticks.max(1)).ok())
            .ok_or(AiError::TimeTooLarge)?;
        let mut host = world.non_send_mut::<ScriptHost>();
        let script = host.compile(source).map_err(AiError::Script)?;
        if !host.defines(script, Hook::Think.name(), Hook::Think.params()) {
            return Err(AiError::NoThink);
        }
        world
            .resource_mut::<AiBook>()
            .set(unit_type, Ai { script, period });
        Ok(())
    }
}

/// Makes each order the current one of its unit, in input order, so a later order in the tick
/// wins. An order to a unit its player does not control, or that is dead, is ignored, and so are
/// a body that is not an order, a target beyond the world bound, and an attack on a unit that is
/// not a living enemy: a client can send anything. A move cancels an attack in its windup, and
/// so does an attack on another target. A cast replaces a cast not resolved yet; its checks run
/// in Act.
fn apply_orders(
    inputs: Res<'_, TickInputs>,
    index: Res<'_, EntityIndex>,
    targets: Targets<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            &Controller,
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
            let Some(Ok((controller, position, team, destination, attack, slots))) =
                index.get(order.unit).map(|entity| units.get_mut(entity))
            else {
                continue;
            };
            if controller.slot() != input.slot {
                continue;
            }
            match order.action {
                Action::Move { x, z } => {
                    // Orders name a point on the ground plane; the unit keeps its height.
                    let (Some(mut destination), Some(target)) = (
                        destination,
                        Position::new(Vec3::new(x, position.get().y, z)),
                    ) else {
                        continue;
                    };
                    destination.set(Some(target));
                    if let Some(mut attack) = attack {
                        attack.set_target(None);
                    }
                }
                Action::Attack { target } => {
                    let enemy = team.is_some_and(|&team| targets.enemy_at(team, target).is_some());
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
    since: u64,
    id: StableId,
    entity: Entity,
    script: ScriptId,
    period: u64,
}

/// Runs `think` for each living unit of a type with AI that is due, those due longest first,
/// then by stable id. A unit is first due in the first tick that leaves the remainder of its
/// stable id when divided by its type's period, so the units of a type spread over the period;
/// then a period after each think. Each call's orders apply when it returns; a failed call's do
/// not. A unit whose call finds the think pool spent stays due, so under load AI thinks later,
/// and no unit misses its turn for good.
fn think(world: &mut World, mut due: Local<'_, Vec<Due>>) {
    let now = world.resource::<SimTick>().get();
    due.clear();
    let book = world.resource::<AiBook>();
    for (id, entity) in world.resource::<EntityIndex>().iter() {
        let unit = world.entity(entity);
        let ai = unit
            .get::<UnitType>()
            .and_then(|&unit_type| book.get(unit_type));
        let Some(ai) = ai.filter(|_| !unit.contains::<Dead>()) else {
            continue;
        };
        let period = u64::from(ai.period);
        let since = match unit.get::<NextThink>() {
            Some(next) => next.get(),
            None if now % period == id.get() % period => now,
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
    ctx.view().read(world);
    let mut host = world
        .remove_non_send::<ScriptHost>()
        .expect("units are installed");
    let mut budget = world.resource::<ScriptBudgets>().think;
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
        let next = match host.call(&mut budget, script, Hook::Think.name(), (ctx.clone(), unit)) {
            Ok(_) => {
                for order in ctx.frame().orders.drain(..) {
                    apply_ai_order(world, id, order);
                }
                now + period
            }
            Err(ScriptError::TickBudget) => since,
            Err(error) => {
                world
                    .non_send_mut::<ScriptFailures>()
                    .0
                    .push(ScriptFailure {
                        unit: id,
                        hook: Hook::Think,
                        error: CallError::from_script(error),
                    });
                now + period
            }
        };
        world.entity_mut(entity).insert(NextThink::new(next));
    }
    world.resource_mut::<ScriptBudgets>().think = budget;
    world.insert_non_send(host);
}

/// Applies an order the AI call of `unit` queued, which the call checked against the units as
/// the phase began; no unit dies within Think.
fn apply_ai_order(world: &mut World, unit: StableId, order: AiOrder) {
    let target = match order {
        AiOrder::Attack { target } => Some(target),
        AiOrder::FollowLane => None,
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
/// once it stands on one. A walker that chased a target walks back to where it left its path.
fn follow_paths(
    lanes: Res<'_, Lanes>,
    mut walkers: Query<
        '_,
        '_,
        (&Position, &mut LaneWalker, &AttackState, &mut Destination),
        Without<Dead>,
    >,
) {
    for (&position, mut walker, attack, mut destination) in &mut walkers {
        if attack.target().is_some() {
            continue;
        }
        let mut waypoint = lanes.waypoint(walker.lane(), walker.next(), walker.direction());
        if waypoint == Some(position) {
            walker.advance();
            waypoint = lanes.waypoint(walker.lane(), walker.next(), walker.direction());
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
        ),
        Without<Dead>,
    >,
) {
    for (&position, &team, stats, mut attack, mut destination) in &mut chasers {
        let Some(target) = attack.target() else {
            continue;
        };
        if attack.started().is_some() {
            continue;
        }
        match targets.enemy_at(team, target) {
            None => {
                attack.set_target(None);
                walk_to(&mut destination, None);
            }
            Some(at) if stats.reaches(position, at) => walk_to(&mut destination, None),
            Some(at) => walk_to(&mut destination, Some(at)),
        }
    }
}

/// Sets where a unit walks, leaving a destination that does not change untouched: a write marks
/// it changed, and a hero's destination replicates.
fn walk_to(destination: &mut Mut<'_, Destination>, target: Option<Position>) {
    if destination.get() != target {
        destination.set(target);
    }
}

#[cfg(test)]
mod tests;
