use crate::combat::CombatSet;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::targets::Targets;
use crate::combat::team::Team;
use crate::navigation::destination::Destination;
use crate::navigation::lane_walker::LaneWalker;
use crate::navigation::lanes::Lanes;
use bevy_ecs::query::{With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::Mut;
use campfire_math::Vec3;
use campfire_sim::{Command, EntityIndex, Position, SimSet, StateRegistry, TickInputs};

use crate::control::controller::Controller;
use crate::control::order::{Action, Order};
use crate::control::tower_ai::TowerAi;

pub(crate) mod controller;
pub(crate) mod order;
pub(crate) mod tower_ai;

/// The `control` capability: who moves a unit. For now, of the orders kind: units that take
/// orders from a player, and towers that choose their own targets.
#[derive(Debug)]
pub struct Control;

impl Control {
    /// Adds control to a match: in Inputs, orders become current; in Think, towers choose
    /// targets; in Act, before combat starts attacks, units walk their paths and chase their
    /// targets. It builds on combat and navigation, which a match installs too.
    pub fn install(schedule: &mut Schedule, registry: &mut StateRegistry) {
        schedule.add_systems((
            apply_orders.in_set(SimSet::Inputs),
            choose_tower_targets.in_set(SimSet::Think),
            (follow_paths, chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        registry.register_component::<Controller>();
        registry.register_component::<TowerAi>();
    }
}

/// Makes each order the current one of its unit, in input order, so a later order in the tick
/// wins. An order to a unit its player does not control, or that is dead, is ignored, and so are
/// a body that is not an order, a target beyond the world bound, and an attack on a unit that is
/// not a living enemy: a client can send anything. A move cancels an attack in its windup, and
/// so does an attack on another target.
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
        ),
        Without<Dead>,
    >,
) {
    for input in inputs.iter() {
        for body in Command::bodies(input.payload, Order::CAPABILITY) {
            let Some(order) = Order::decode(body) else {
                continue;
            };
            let Some(Ok((controller, position, team, destination, attack))) =
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
            }
        }
    }
}

/// Keeps each tower's target while it lives and stays in range, and otherwise takes the nearest
/// enemy in range, the lower stable id on a tie. A tower in its windup keeps its target. It
/// stands in for the tower AI script until scripts run.
fn choose_tower_targets(
    targets: Targets<'_, '_>,
    mut towers: Query<
        '_,
        '_,
        (&Position, &Team, &AttackStats, &mut AttackState),
        (With<TowerAi>, Without<Dead>),
    >,
) {
    for (&position, &team, stats, mut attack) in &mut towers {
        if attack.started().is_some() {
            continue;
        }
        let kept = attack
            .target()
            .and_then(|target| targets.enemy_at(team, target))
            .is_some_and(|at| stats.reaches(position, at));
        if !kept {
            attack.set_target(targets.nearest_enemy(team, position, stats));
        }
    }
}

/// Sends each path walker with no attack target to its path's next waypoint once it reached the
/// one before.
fn follow_paths(
    lanes: Res<'_, Lanes>,
    mut walkers: Query<'_, '_, (&mut LaneWalker, &AttackState, &mut Destination), Without<Dead>>,
) {
    for (mut walker, attack, mut destination) in &mut walkers {
        if attack.target().is_some() || destination.get().is_some() {
            continue;
        }
        if let Some(waypoint) = lanes.waypoint(walker.lane(), walker.next(), walker.direction()) {
            destination.set(Some(waypoint));
            walker.advance();
        }
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
