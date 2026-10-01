use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::{EntityRef, World};
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry};

use crate::combat::assist_window::AssistWindow;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::dead::Dead;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::health::Health;
use crate::combat::launches::{Launch, Launches};
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::strikes::{Strike, Strikes};
use crate::combat::targets::Targets;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::recent_attack::RecentAttack;
use crate::units::script_view::{RowFill, View};
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;

pub(crate) mod assist_window;
pub(crate) mod attack_state;
pub(crate) mod attack_stats;
pub(crate) mod combat_data;
pub(crate) mod combatant;
pub(crate) mod dead;
pub(crate) mod deaths;
pub(crate) mod health;
pub(crate) mod launches;
pub(crate) mod on_death;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod strikes;
pub(crate) mod targets;

/// The `combat` capability: teams, health, attacks, damage and deaths.
#[derive(Debug)]
pub struct Combat;

/// Combat's systems, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CombatSet {
    /// In `SimSet::Inputs`: dead units whose respawn is due come back, before any input of the
    /// tick can reach them.
    Respawn,
    /// In `SimSet::Act`: attacks start, and targets that are gone are dropped.
    Attack,
    /// In `SimSet::Hit`: windups that end strike, or fire.
    Strike,
    /// In `SimSet::Hit`, after `Strike`: the tick's launches take off.
    Launch,
}

impl Combat {
    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the strikes deal their damage and each target
    /// records its attacker, then units at zero health die, each with its killer and assisters;
    /// in Vision, the dead whose type despawns go, after the Mode stage saw them.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(Strikes::default());
        world.insert_resource(Deaths::default());
        schedule.configure_sets(
            CombatSet::Launch
                .in_set(SimSet::Hit)
                .after(CombatSet::Strike),
        );
        schedule.add_systems((
            attack.in_set(SimSet::Act).in_set(CombatSet::Attack),
            strike.in_set(SimSet::Hit).in_set(CombatSet::Strike),
            (apply_strikes, die).chain().in_set(SimSet::Resolve),
            respawn.in_set(SimSet::Inputs).in_set(CombatSet::Respawn),
            despawn_dead.in_set(SimSet::Vision),
        ));
        registry.register_component::<AttackState>();
        registry.register_component::<AttackStats>();
        registry.register_component::<Dead>();
        registry.register_component::<Health>();
        registry.register_component::<OnDeath>();
        registry.register_component::<RecentAttackers>();
        registry.register_component::<Respawn>();
    }
}

/// Fills a row of the script view with what combat holds: whether the unit lives and whether it
/// stays when dead, its attack's target and range, and who struck it recently.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.alive = !unit.contains::<Dead>();
    fill.row.stays = unit.get::<OnDeath>() == Some(&OnDeath::Stay);
    fill.row.target = unit.get::<AttackState>().and_then(|attack| attack.target());
    fill.row.attack_range = unit.get::<AttackStats>().map(|stats| stats.range());
    fill.row.health = unit.get::<Health>().copied();
    if let Some(recent) = unit.get::<RecentAttackers>() {
        fill.attacked(recent.iter());
    }
}

/// Drops each target that is gone, dead or no longer an enemy, which cancels its windup, and
/// starts an attack when the target is in range and the unit is ready. The range counts only at
/// the start.
fn attack(
    tick: Res<'_, SimTick>,
    targets: Targets<'_, '_>,
    mut attackers: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &AttackStats,
            &mut AttackState,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, stats, mut attack, body) in &mut attackers {
        let Some(target) = attack.target() else {
            continue;
        };
        let Some(target) = targets.enemy(team, target) else {
            attack.set_target(None);
            continue;
        };
        if attack.started().is_none()
            && now >= attack.ready_at()
            && stats.reaches(position, Body::radius_of(body), &target)
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
    let now = tick.start();
    for (&source, &from, stats, mut attack) in &mut attackers {
        let Some(started) = attack.started() else {
            continue;
        };
        if now < started.after(stats.windup()) {
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
        attack.strike(started.after(stats.period()));
    }
}

/// Deals the tick's strikes in the order of their source's stable id, and records each source
/// with its target. The strike that takes a unit to zero health makes its source the killer, when
/// the source still exists: a projectile can outlive the unit that launched it. The other units
/// that struck it within the assist window assisted, by stable id.
fn apply_strikes(
    tick: Res<'_, SimTick>,
    index: Res<'_, EntityIndex>,
    window: Option<Res<'_, AssistWindow>>,
    mut strikes: ResMut<'_, Strikes>,
    mut deaths: ResMut<'_, Deaths>,
    mut targets: Query<
        '_,
        '_,
        (
            &mut Health,
            Option<&mut RecentAttackers>,
            Option<&Team>,
            Option<&Owner>,
        ),
    >,
) {
    let now = tick.start();
    deaths.clear(now);
    strikes.0.sort_unstable_by_key(|strike| strike.source);
    for strike in strikes.0.drain(..) {
        let Some((mut health, mut attackers, team, owner)) = index
            .get(strike.target)
            .and_then(|entity| targets.get_mut(entity).ok())
        else {
            continue;
        };
        let alive = !health.is_zero();
        health.take(strike.amount);
        if let Some(attackers) = attackers.as_deref_mut() {
            attackers.record(strike.source, now, &index);
        }
        if !alive || !health.is_zero() {
            continue;
        }
        let assisted = |attack: &RecentAttack| {
            let since = now.since(attack.tick);
            let within = window.as_ref().zip(since).is_some_and(|(w, s)| s <= w.0);
            attack.source != strike.source && within
        };
        let assisters = attackers
            .as_deref()
            .into_iter()
            .flat_map(RecentAttackers::iter)
            .filter(assisted)
            .map(|attack| attack.source);
        let killer = Some(strike.source).filter(|&source| index.get(source).is_some());
        deaths.push(Fallen::of(strike.target, team, owner), killer, assisters);
    }
}

/// A unit at zero health dies, and the Mode stage learns of it; one no strike took there died
/// with no killer.
fn die(
    mut commands: Commands<'_, '_>,
    mut deaths: ResMut<'_, Deaths>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Health,
            Option<&mut AttackState>,
            Option<&Team>,
            Option<&Owner>,
        ),
        (With<OnDeath>, Without<Dead>),
    >,
) {
    for (entity, &id, health, attack, team, owner) in &mut units {
        if !health.is_zero() {
            continue;
        }
        if let Some(mut attack) = attack {
            attack.set_target(None);
        }
        commands.entity(entity).insert(Dead);
        if !deaths.contains(id) {
            deaths.push(Fallen::of(id, team, owner), None, []);
        }
    }
}

/// Despawns the dead whose unit type despawns, at the end of the tick they died in.
fn despawn_dead(
    mut commands: Commands<'_, '_>,
    dead: Query<'_, '_, (Entity, &OnDeath), With<Dead>>,
) {
    for (entity, &on_death) in &dead {
        if on_death == OnDeath::Despawn {
            commands.entity(entity).despawn();
        }
    }
}

/// Brings back each dead unit whose respawn is due, at its spawn point with full health and
/// no one on record as its attacker.
fn respawn(
    tick: Res<'_, SimTick>,
    mut commands: Commands<'_, '_>,
    mut dead: Query<
        '_,
        '_,
        (
            Entity,
            &Respawn,
            &SpawnPoint,
            &mut Health,
            &mut Position,
            Option<&mut RecentAttackers>,
        ),
        With<Dead>,
    >,
) {
    let now = tick.start();
    for (entity, respawn, spawn, mut health, mut position, attackers) in &mut dead {
        if respawn.at > now {
            continue;
        }
        health.fill();
        *position = spawn.get();
        if let Some(mut attackers) = attackers {
            *attackers = RecentAttackers::default();
        }
        commands.entity(entity).remove::<(Dead, Respawn)>();
    }
}

#[cfg(test)]
mod tests;
