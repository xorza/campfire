use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry};

use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::launches::{Launch, Launches};
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::strikes::{Strike, Strikes};
use crate::combat::targets::Targets;
use crate::combat::team::Team;

pub(crate) mod attack_state;
pub(crate) mod attack_stats;
pub(crate) mod combatant;
pub(crate) mod damage_kind;
pub(crate) mod dead;
pub(crate) mod health;
pub(crate) mod launches;
pub(crate) mod living_unit;
pub(crate) mod on_death;
pub(crate) mod recent_attackers;
pub(crate) mod strikes;
pub(crate) mod targets;
pub(crate) mod team;

/// The `combat` capability: teams, health, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

/// Combat's systems, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CombatSet {
    /// In `SimSet::Act`: attacks start, and targets that are gone are dropped.
    Attack,
    /// In `SimSet::Hit`: windups that end strike, or fire.
    Strike,
    /// In `SimSet::Hit`, after `Strike`: the tick's launches take off.
    Launch,
}

impl Combat {
    /// Adds combat to a match: in Act, attacks in range start once ready; in Hit, windups that
    /// end strike, or fire when ranged and the match has projectiles; in Resolve, the strikes
    /// deal their damage and each target records its attacker, then units at zero health die.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(Strikes::default());
        schedule.configure_sets(
            CombatSet::Launch
                .in_set(SimSet::Hit)
                .after(CombatSet::Strike),
        );
        schedule.add_systems((
            attack.in_set(SimSet::Act).in_set(CombatSet::Attack),
            strike.in_set(SimSet::Hit).in_set(CombatSet::Strike),
            (apply_strikes, die).chain().in_set(SimSet::Resolve),
        ));
        registry.register_component::<AttackState>();
        registry.register_component::<AttackStats>();
        registry.register_component::<Dead>();
        registry.register_component::<Health>();
        registry.register_component::<OnDeath>();
        registry.register_component::<RecentAttackers>();
        registry.register_component::<Team>();
    }
}

/// Drops each target that is gone, dead or no longer an enemy, which cancels its windup, and
/// starts an attack when the target is in range and the unit is ready. The range counts only at
/// the start.
fn attack(
    tick: Res<'_, SimTick>,
    targets: Targets<'_, '_>,
    mut attackers: Query<'_, '_, (&Position, &Team, &AttackStats, &mut AttackState), Without<Dead>>,
) {
    let now = tick.get();
    for (&position, &team, stats, mut attack) in &mut attackers {
        let Some(target) = attack.target() else {
            continue;
        };
        let Some(target_at) = targets.enemy_at(team, target) else {
            attack.set_target(None);
            continue;
        };
        if attack.started().is_none()
            && now >= attack.ready_at()
            && stats.reaches(position, target_at)
        {
            attack.start(now);
        }
    }
}

/// Queues the strike of each attack whose windup ends this tick, or its launch when it is ranged
/// and the match has projectiles.
fn strike(
    tick: Res<'_, SimTick>,
    mut strikes: ResMut<'_, Strikes>,
    mut launches: Option<ResMut<'_, Launches>>,
    mut attackers: Query<
        '_,
        '_,
        (&StableId, &Position, &AttackStats, &mut AttackState),
        Without<Dead>,
    >,
) {
    let now = tick.get();
    for (&source, &from, stats, mut attack) in &mut attackers {
        let Some(started) = attack.started() else {
            continue;
        };
        if now < after(started, stats.windup()) {
            continue;
        }
        let target = attack
            .target()
            .expect("an attack in its windup has a target");
        let amount = stats.damage();
        match (stats.projectile_speed(), launches.as_deref_mut()) {
            (Some(speed), Some(launches)) => launches.0.push(Launch {
                source,
                from,
                target,
                amount,
                speed,
            }),
            _ => strikes.0.push(Strike {
                source,
                target,
                amount,
            }),
        }
        attack.strike(after(started, stats.period()));
    }
}

/// Deals the tick's strikes in the order of their source's stable id, and records each source
/// with its target.
fn apply_strikes(
    tick: Res<'_, SimTick>,
    index: Res<'_, EntityIndex>,
    mut strikes: ResMut<'_, Strikes>,
    mut targets: Query<'_, '_, (&mut Health, Option<&mut RecentAttackers>)>,
) {
    let now = tick.get();
    strikes.0.sort_unstable_by_key(|strike| strike.source);
    for strike in strikes.0.drain(..) {
        let Some((mut health, attackers)) = index
            .get(strike.target)
            .and_then(|entity| targets.get_mut(entity).ok())
        else {
            continue;
        };
        health.take(strike.amount);
        if let Some(mut attackers) = attackers {
            attackers.record(strike.source, now, &index);
        }
    }
}

/// A unit at zero health dies: it stays, dead, or despawns, as its unit type says.
fn die(
    mut commands: Commands<'_, '_>,
    mut units: Query<'_, '_, (Entity, &Health, &OnDeath, &mut AttackState), Without<Dead>>,
) {
    for (entity, health, on_death, mut attack) in &mut units {
        if !health.is_zero() {
            continue;
        }
        match on_death {
            OnDeath::Stay => {
                attack.set_target(None);
                commands.entity(entity).insert(Dead);
            }
            OnDeath::Despawn => commands.entity(entity).despawn(),
        }
    }
}

/// `tick` plus `ticks`.
fn after(tick: u64, ticks: u32) -> u64 {
    tick.checked_add(u64::from(ticks))
        .expect("tick numbers exhausted")
}

#[cfg(test)]
mod tests;
