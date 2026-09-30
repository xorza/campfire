use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_math::{Num, Vec3};
use campfire_sim::{
    EntityIndex, IdAllocator, Position, SimSet, SimTick, StableId, StateRegistry, TickInputs,
};

use crate::attack_state::AttackState;
use crate::attack_stats::AttackStats;
use crate::controller::Controller;
use crate::dead::Dead;
use crate::destination::Destination;
use crate::health::Health;
use crate::lane_walker::LaneWalker;
use crate::lanes::Lanes;
use crate::move_step::MoveStep;
use crate::order::Order;
use crate::strikes::{Strike, Strikes};
use crate::team::Team;
use crate::tower_ai::TowerAi;
use crate::unit_stats::UnitStats;
use crate::waves::Waves;

/// Wires the MOBA kit into a sim: its systems, its state types, and its units.
#[derive(Debug)]
pub struct MobaKit;

impl MobaKit {
    /// The kit's steps of a tick. Before collision: towers choose targets, creeps walk their
    /// lanes, attacks chase, wind up and strike, units move. After collision: strikes deal their
    /// damage, then units at zero health die. In the mode step, due creep waves spawn.
    pub fn add_systems(schedule: &mut Schedule) {
        schedule.add_systems((
            apply_orders.in_set(SimSet::Inputs),
            (choose_tower_targets, walk_lanes, attack, move_units)
                .chain()
                .in_set(SimSet::BeforeCollision),
            (apply_strikes, die).chain().in_set(SimSet::AfterCollision),
            spawn_waves.in_set(SimSet::Mode),
        ));
    }

    pub fn register_state(registry: &mut StateRegistry) {
        registry.register_component::<AttackState>();
        registry.register_component::<AttackStats>();
        registry.register_component::<Controller>();
        registry.register_component::<Dead>();
        registry.register_component::<Destination>();
        registry.register_component::<Health>();
        registry.register_component::<LaneWalker>();
        registry.register_component::<MoveStep>();
        registry.register_component::<Team>();
        registry.register_component::<TowerAi>();
    }

    /// Inserts what the kit's steps need into a world `SimUpdate::prepare` set up: the map's
    /// `lanes` and, where the match has them, its creep `waves`. A predicting client, which never
    /// spawns units, passes none.
    pub fn prepare(world: &mut World, lanes: Lanes, waves: Option<Waves>) {
        world.insert_resource(Strikes::default());
        world.insert_resource(lanes);
        if let Some(waves) = waves {
            world.insert_resource(waves);
        }
    }

    /// Spawns a hero that follows the orders of the player in `slot`.
    pub fn spawn_hero(
        world: &mut World,
        slot: u32,
        team: Team,
        at: Position,
        stats: UnitStats,
    ) -> StableId {
        MobaKit::spawn_unit(world, team, at, stats, Controller::new(slot))
    }

    /// Spawns a tower, which chooses its own targets.
    pub fn spawn_tower(world: &mut World, team: Team, at: Position, stats: UnitStats) -> StableId {
        MobaKit::spawn_unit(world, team, at, stats, TowerAi)
    }

    /// Spawns a wave of `creeps` for `team` at the start of `lane`, in order, each walking the
    /// lane.
    pub fn spawn_wave(world: &mut World, team: Team, lane: u32, creeps: &[UnitStats]) {
        let start = world
            .resource::<Lanes>()
            .waypoint(lane, 0, team)
            .expect("a wave spawns for a side, on a lane of the map");
        for &stats in creeps {
            MobaKit::spawn_unit(world, team, start, stats, LaneWalker::new(lane, 1));
        }
    }

    /// Spawns a unit with `role`, the components that make it a hero, a tower or a creep.
    fn spawn_unit(
        world: &mut World,
        team: Team,
        at: Position,
        stats: UnitStats,
        role: impl Bundle,
    ) -> StableId {
        let id = world.resource_mut::<IdAllocator>().allocate();
        let mut unit = world.spawn((
            id,
            at,
            team,
            stats.health,
            stats.attack,
            AttackState::default(),
            role,
        ));
        if let Some(step) = stats.step {
            unit.insert((Destination::default(), step));
        }
        id
    }
}

/// Makes each tick input the current order of its player's living units, in input order, so a
/// later order in the tick wins. A payload that is not an order, a target beyond the world bound,
/// and an attack on a unit that is not a living enemy are ignored: a client can send anything.
/// A move cancels an attack in its windup, and so does an attack on another target.
fn apply_orders(
    inputs: Res<'_, TickInputs>,
    index: Res<'_, EntityIndex>,
    mut units: Query<
        '_,
        '_,
        (
            &Controller,
            &Position,
            &mut Destination,
            Option<&Team>,
            Option<&mut AttackState>,
        ),
        Without<Dead>,
    >,
    targets: Query<'_, '_, &Team, (With<Health>, Without<Dead>)>,
) {
    for input in inputs.iter() {
        let Some(order) = Order::decode(input.payload) else {
            continue;
        };
        for (controller, position, mut destination, team, attack) in &mut units {
            if controller.slot() != input.slot {
                continue;
            }
            match order {
                Order::Move { x, z } => {
                    // Orders name a point on the ground plane; the unit keeps its height.
                    let Some(target) = Position::new(Vec3::new(x, position.get().y, z)) else {
                        continue;
                    };
                    destination.set(Some(target));
                    if let Some(mut attack) = attack {
                        attack.set_target(None);
                    }
                }
                Order::Attack { target } => {
                    let enemy = index
                        .get(target)
                        .and_then(|entity| targets.get(entity).ok())
                        .zip(team)
                        .is_some_and(|(theirs, ours)| ours.is_enemy_of(*theirs));
                    if let (true, Some(mut attack)) = (enemy, attack) {
                        attack.set_target(Some(target));
                    }
                }
            }
        }
    }
}

/// Keeps each tower's target while it lives and stays in range, and otherwise takes the nearest
/// enemy in range, the lower stable id on a tie. A tower in its windup keeps its target.
fn choose_tower_targets(
    index: Res<'_, EntityIndex>,
    mut towers: Query<
        '_,
        '_,
        (&Position, &Team, &AttackStats, &mut AttackState),
        (With<TowerAi>, Without<Dead>),
    >,
    units: Query<'_, '_, (&StableId, &Position, &Team), (With<Health>, Without<Dead>)>,
) {
    for (position, team, stats, mut attack) in &mut towers {
        if attack.started().is_some() {
            continue;
        }
        let reachable = |(theirs, at): (&Team, &Position)| {
            team.is_enemy_of(*theirs) && in_range(*position, *at, stats.range())
        };
        let kept = attack
            .target()
            .and_then(|target| index.get(target))
            .and_then(|entity| units.get(entity).ok())
            .is_some_and(|(_, at, theirs)| reachable((theirs, at)));
        if kept {
            continue;
        }
        let nearest = units
            .iter()
            .filter(|&(_, at, theirs)| reachable((theirs, at)))
            .min_by_key(|&(id, at, _)| (ground_distance_squared(*position, *at), *id))
            .map(|(id, _, _)| *id);
        attack.set_target(nearest);
    }
}

/// Sends each creep with no attack target to its lane's next waypoint once it reached the one
/// before.
fn walk_lanes(
    lanes: Res<'_, Lanes>,
    mut creeps: Query<
        '_,
        '_,
        (&mut LaneWalker, &Team, &AttackState, &mut Destination),
        Without<Dead>,
    >,
) {
    for (mut walker, team, attack, mut destination) in &mut creeps {
        if attack.target().is_some() || destination.get().is_some() {
            continue;
        }
        if let Some(waypoint) = lanes.waypoint(walker.lane(), walker.next(), *team) {
            destination.set(Some(waypoint));
            walker.advance();
        }
    }
}

/// Runs each unit's attack. A unit whose target is gone, dead or no longer an enemy drops it and
/// stops. Out of range it walks to the target, if it can move; in range it stops, and starts an
/// attack once ready. An attack strikes when its windup ends, unless its target is gone by
/// then; the range counts only at the start.
fn attack(
    tick: Res<'_, SimTick>,
    index: Res<'_, EntityIndex>,
    mut strikes: ResMut<'_, Strikes>,
    mut attackers: Query<
        '_,
        '_,
        (
            &StableId,
            &Position,
            &Team,
            &AttackStats,
            &mut AttackState,
            Option<&mut Destination>,
        ),
        Without<Dead>,
    >,
    targets: Query<'_, '_, (&Position, &Team), (With<Health>, Without<Dead>)>,
) {
    let now = tick.get();
    for (&id, &position, team, stats, mut attack, mut destination) in &mut attackers {
        let Some(target) = attack.target() else {
            continue;
        };
        let target_at = index
            .get(target)
            .and_then(|entity| targets.get(entity).ok())
            .filter(|(_, theirs)| team.is_enemy_of(**theirs))
            .map(|(at, _)| *at);
        let Some(target_at) = target_at else {
            attack.set_target(None);
            walk_to(destination.as_mut(), None);
            continue;
        };
        if let Some(started) = attack.started() {
            if now >= after(started, stats.windup()) {
                strike(&mut strikes, id, target, stats, &mut attack, started);
            }
            continue;
        }
        if !in_range(position, target_at, stats.range()) {
            walk_to(destination.as_mut(), Some(target_at));
            continue;
        }
        walk_to(destination.as_mut(), None);
        if now < attack.ready_at() {
            continue;
        }
        if stats.windup() == 0 {
            strike(&mut strikes, id, target, stats, &mut attack, now);
        } else {
            attack.start(now);
        }
    }
}

/// Sets where a unit that can move walks, leaving a destination that does not change untouched:
/// a write marks it changed, and a hero's destination replicates.
fn walk_to(destination: Option<&mut Mut<'_, Destination>>, target: Option<Position>) {
    if let Some(destination) = destination
        && destination.get() != target
    {
        destination.set(target);
    }
}

/// Queues the strike of the attack `source` started in tick `started`.
fn strike(
    strikes: &mut Strikes,
    source: StableId,
    target: StableId,
    stats: &AttackStats,
    attack: &mut AttackState,
    started: u64,
) {
    strikes.0.push(Strike {
        source,
        target,
        amount: stats.damage(),
    });
    attack.strike(after(started, stats.period()));
}

fn move_units(
    mut units: Query<'_, '_, (&mut Position, &mut Destination, &MoveStep), Without<Dead>>,
) {
    for (mut position, mut destination, step) in &mut units {
        let Some(target) = destination.get() else {
            continue;
        };
        let moved = position.get().step_toward(target.get(), step.get());
        *position = Position::new(moved).expect("a step ends between two points within the bound");
        if moved == target.get() {
            destination.set(None);
        }
    }
}

/// Deals the tick's strikes in the order of their source's stable id.
fn apply_strikes(
    index: Res<'_, EntityIndex>,
    mut strikes: ResMut<'_, Strikes>,
    mut healths: Query<'_, '_, &mut Health>,
) {
    strikes.0.sort_unstable_by_key(|strike| strike.source);
    for strike in strikes.0.drain(..) {
        if let Some(mut health) = index
            .get(strike.target)
            .and_then(|entity| healths.get_mut(entity).ok())
        {
            health.take(strike.amount);
        }
    }
}

/// A hero at zero health dies and stays, dead, to respawn; any other unit despawns.
fn die(
    mut commands: Commands<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &Health,
            Has<Controller>,
            &mut AttackState,
            Option<&mut Destination>,
        ),
        Without<Dead>,
    >,
) {
    for (entity, health, hero, mut attack, destination) in &mut units {
        if !health.is_zero() {
            continue;
        }
        if hero {
            attack.set_target(None);
            if let Some(mut destination) = destination {
                destination.set(None);
            }
            commands.entity(entity).insert(Dead);
        } else {
            commands.entity(entity).despawn();
        }
    }
}

/// Spawns the waves due this tick: on each lane in order, the first side's, then the second's.
fn spawn_waves(world: &mut World) {
    if !world.contains_resource::<Waves>() {
        return;
    }
    let tick = world.resource::<SimTick>().get();
    world.resource_scope(|world, waves: Mut<'_, Waves>| {
        if !waves.due(tick) {
            return;
        }
        for lane in 0..world.resource::<Lanes>().count() {
            for team in [Team::First, Team::Second] {
                MobaKit::spawn_wave(world, team, lane, &waves.creeps);
            }
        }
    });
}

/// `tick` plus `ticks`.
fn after(tick: u64, ticks: u32) -> u64 {
    tick.checked_add(u64::from(ticks))
        .expect("tick numbers exhausted")
}

/// On the ground plane: heights never count towards a range.
fn ground_offset(from: Position, to: Position) -> Vec3 {
    let offset = to.get() - from.get();
    Vec3::new(offset.x, Num::ZERO, offset.z)
}

fn ground_distance_squared(from: Position, to: Position) -> u128 {
    ground_offset(from, to).length_squared_bits()
}

fn in_range(from: Position, to: Position, range: Num) -> bool {
    Vec3::ZERO.within(ground_offset(from, to), range)
}

#[cfg(test)]
mod tests;
