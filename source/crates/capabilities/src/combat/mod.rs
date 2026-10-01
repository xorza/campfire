use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_math::Num;
use campfire_script::ScriptHost;
use campfire_sim::{EntityIndex, Position, SimRng, SimSet, SimTick, StableId, StateRegistry, Tick};

use crate::abilities::resource_pool::ResourcePool;
use crate::combat::assist_window::AssistWindow;
use crate::combat::attack_kind::AttackKind;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_handle::DamageHandle;
use crate::combat::damage_queue::DamageQueue;
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::dead::Dead;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::health::Health;
use crate::combat::launches::{Launch, Launches};
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::targets::Targets;
use crate::scripts::hook::Hook;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat::EngineStat;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::recent_attack::RecentAttack;
use crate::units::script_view::{RowFill, View};
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;

pub(crate) mod assist_window;
pub(crate) mod attack_kind;
pub(crate) mod attack_state;
pub(crate) mod attack_stats;
pub(crate) mod combat_data;
pub(crate) mod combatant;
pub(crate) mod damage;
pub(crate) mod damage_handle;
pub(crate) mod damage_kind;
pub(crate) mod damage_queue;
pub(crate) mod damage_weigher;
pub(crate) mod dead;
pub(crate) mod deaths;
pub(crate) mod health;
pub(crate) mod launches;
pub(crate) mod on_death;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod targets;

/// The random stream an attack's crit draws from, for its attacker in its tick.
const CRIT_STREAM: &str = "combat.crit";

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
    /// In `SimSet::Hit`: windups that end strike, or fire, each with its crit rolled.
    Strike,
    /// In `SimSet::Hit`, after `Strike`: the tick's launches take off.
    Launch,
    /// In `SimSet::Resolve`: the tick's damage is dealt.
    Damage,
    /// In `SimSet::Resolve`, after `Damage`: units at zero health die.
    Die,
}

impl Combat {
    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the tick's damage is dealt, then units at zero
    /// health die, each with its killer and assisters; in Vision, the dead whose type despawns
    /// go, after the Mode stage saw them.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        if let Some(mut host) = world.get_non_send_mut::<ScriptHost>() {
            DamageHandle::register(host.engine_mut());
        }
        world.insert_resource(DamageQueue::default());
        world.insert_resource(AttackKind::default());
        world.insert_resource(Deaths::default());
        schedule.configure_sets(
            CombatSet::Launch
                .in_set(SimSet::Hit)
                .after(CombatSet::Strike),
        );
        schedule.add_systems((
            attack.in_set(SimSet::Act).in_set(CombatSet::Attack),
            strike.in_set(SimSet::Hit).in_set(CombatSet::Strike),
            (
                deal_damage.in_set(CombatSet::Damage),
                die.in_set(CombatSet::Die),
            )
                .chain()
                .in_set(SimSet::Resolve),
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

/// Queues the damage of each attack whose windup ends this tick, or its launch when it is ranged
/// and the match has projectiles. Each rolls its crit now, with its attacker's `crit_chance`.
fn strike(
    (tick, rng, kind, book): (
        Res<'_, SimTick>,
        Res<'_, SimRng>,
        Res<'_, AttackKind>,
        Option<Res<'_, StatBook>>,
    ),
    mut queue: ResMut<'_, DamageQueue>,
    mut launches: Option<ResMut<'_, Launches>>,
    mut attackers: Query<
        '_,
        '_,
        (
            &StableId,
            &Position,
            &AttackStats,
            &mut AttackState,
            Option<&UnitStats>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&source, &from, stats, mut attack, unit_stats) in &mut attackers {
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
        let chance = book
            .as_deref()
            .zip(unit_stats)
            .and_then(|(book, unit)| book.engine(unit.values(), EngineStat::CritChance))
            .unwrap_or(Num::ZERO);
        let crit = chance > Num::ZERO && rng.open(CRIT_STREAM, source).chance(chance);
        match (stats.projectile_speed(), launches.as_deref_mut()) {
            (Some(speed), Some(launches)) => launches.0.push(Launch {
                source,
                from,
                target,
                amount,
                speed,
                crit,
            }),
            _ => queue.push(Damage {
                source: Some(source),
                target,
                amount,
                kind: kind.0,
                cause: DamageCause::Attack { crit },
                ability: None,
            }),
        }
        attack.strike(started.after(stats.period()));
    }
}

/// Deals the tick's damage in the queue's order, each through the mode's `calc_damage` when it
/// has one, with the units as the pass began; damage to a unit at zero health does nothing.
fn deal_damage(world: &mut World) {
    let now = world.resource::<SimTick>().start();
    world.resource_mut::<Deaths>().clear(now);
    world.resource_mut::<DamageQueue>().sort();
    if let Some(weigher) = world.remove_non_send::<DamageWeigher>() {
        let view = world.non_send::<View>().clone();
        ScriptBatch::run(world, &view, |batch| {
            let mut at = 0;
            while let Some(damage) = batch.world().resource::<DamageQueue>().get(at) {
                at += 1;
                if Combat::living(batch.world(), damage.target).is_none() {
                    continue;
                }
                let amount = weigher.weigh(batch, damage).unwrap_or_else(|error| {
                    batch.record(Some(damage.target), Hook::CalcDamage, error);
                    damage.amount
                });
                Combat::deal(batch.world(), damage, amount, now);
            }
        });
        world.insert_non_send(weigher);
    } else {
        let mut at = 0;
        while let Some(damage) = world.resource::<DamageQueue>().get(at) {
            at += 1;
            Combat::deal(world, damage, damage.amount, now);
        }
    }
    world.resource_mut::<DamageQueue>().clear();
}

impl Combat {
    /// The entity of `unit`, when it exists and has health above zero: one that damage, heals
    /// and restores reach.
    fn living(world: &World, unit: StableId) -> Option<Entity> {
        let entity = world.resource::<EntityIndex>().get(unit)?;
        let health = world.get::<Health>(entity)?;
        (!health.is_zero()).then_some(entity)
    }

    /// Deals `damage` as `amount` in tick `now`, a negative amount as 0: shields absorb it, then
    /// health takes the rest. The source is recorded as the target's attacker; when the damage
    /// took the target to zero, the source is its killer if it still exists, and the others that
    /// damaged it within the assist window assisted. A living source heals by its life steal,
    /// for an attack, or its spell vamp times the health taken.
    fn deal(world: &mut World, damage: Damage, amount: Num, now: Tick) {
        let Some(entity) = Combat::living(world, damage.target) else {
            return;
        };
        let index = world.resource::<EntityIndex>();
        let source = damage.source.filter(|&source| index.get(source).is_some());
        let mut left = amount.max(Num::ZERO);
        if let Some(mut modifiers) = world.get_mut::<Modifiers>(entity) {
            let after = modifiers.bypass_change_detection().absorb(left);
            if after != left {
                modifiers.set_changed();
            }
            left = after;
        }
        let mut health = world
            .get_mut::<Health>(entity)
            .expect("a unit that takes damage");
        let before = health.current();
        health.take(left);
        let taken = before - health.current();
        let killed = health.is_zero();
        world.resource_scope(|world, index: Mut<'_, EntityIndex>| {
            let mut attackers = world.get_mut::<RecentAttackers>(entity);
            if let (Some(source), Some(attackers)) = (damage.source, attackers.as_deref_mut()) {
                attackers.record(source, now, &index);
            }
        });
        if killed {
            world.resource_scope(|world, mut deaths: Mut<'_, Deaths>| {
                let window = world.get_resource::<AssistWindow>().map(|window| window.0);
                let assisted = |attack: &RecentAttack| {
                    let within = window
                        .zip(now.since(attack.tick))
                        .is_some_and(|(window, since)| since <= window);
                    Some(attack.source) != damage.source && within
                };
                let assisters = world
                    .get::<RecentAttackers>(entity)
                    .into_iter()
                    .flat_map(RecentAttackers::iter)
                    .filter(assisted)
                    .map(|attack| attack.source);
                let fallen = Fallen::of(damage.target, world.get(entity), world.get(entity));
                deaths.push(fallen, source, assisters);
            });
        }
        if let Some(source) = source.and_then(|source| Combat::living(world, source)) {
            let stat = if damage.cause.attack() {
                EngineStat::LifeSteal
            } else {
                EngineStat::SpellVamp
            };
            let ratio = Combat::stat(world, source, stat);
            Combat::heal_living(world, source, scaled(taken, ratio));
        }
    }

    /// Heals `unit` by `amount` times one plus its `healing_received_pct`, when it exists and is
    /// above zero health.
    pub(crate) fn heal(world: &mut World, unit: StableId, amount: Num) {
        if let Some(entity) = Combat::living(world, unit) {
            Combat::heal_living(world, entity, amount);
        }
    }

    /// Heals `entity`, a living unit, as `heal` does.
    fn heal_living(world: &mut World, entity: Entity, amount: Num) {
        let received = Num::ONE + Combat::stat(world, entity, EngineStat::HealingReceivedPct);
        let amount = scaled(amount, received);
        if amount > Num::ZERO {
            let mut health = world.get_mut::<Health>(entity).expect("a living unit");
            health.heal(amount);
        }
    }

    /// Restores `amount` of `unit`'s resource, when it exists and is above zero health.
    pub(crate) fn restore(world: &mut World, unit: StableId, amount: Num) {
        let Some(entity) = Combat::living(world, unit) else {
            return;
        };
        if let Some(mut pool) = world.get_mut::<ResourcePool>(entity)
            && amount > Num::ZERO
        {
            pool.restore(amount);
        }
    }

    /// `entity`'s engine stat `stat`; 0 when the mode does not declare it, or the unit has no
    /// stats.
    fn stat(world: &World, entity: Entity, stat: EngineStat) -> Num {
        let values = world.get::<UnitStats>(entity);
        world
            .get_resource::<StatBook>()
            .zip(values)
            .and_then(|(book, values)| book.engine(values.values(), stat))
            .unwrap_or(Num::ZERO)
    }
}

/// `amount` times `ratio`, rounded once, at the end of the range of numbers when past it.
fn scaled(amount: Num, ratio: Num) -> Num {
    amount.checked_mul(ratio).unwrap_or_else(|| {
        if (amount < Num::ZERO) == (ratio < Num::ZERO) {
            Num::MAX
        } else {
            Num::MIN
        }
    })
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
