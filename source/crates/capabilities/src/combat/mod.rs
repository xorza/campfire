use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::{EntityRef, Mut, World};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, SimRng, SimSet, SimTick, StableId, StateRegistry, Tick};

use crate::abilities::ability_book::AbilityId;
use crate::combat::assist_window::AssistWindow;
use crate::combat::attack_kind::AttackKind;
use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::combat_effect::CombatEffect;
use crate::combat::combat_event::CombatEvent;
use crate::combat::combat_events::CombatEvents;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_queue::DamageQueue;
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::dead::Dead;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::launches::{Launch, Launches};
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::targets::Targets;
use crate::scripts::hook::Hook;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifiers::Modifiers;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::recent_attack::RecentAttack;
use crate::units::script_view::{RowFill, View};
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;

pub(crate) mod assist_window;
pub(crate) mod attack_kind;
pub(crate) mod attack_state;
pub(crate) mod attack_stats;
pub(crate) mod combat_api;
pub(crate) mod combat_bindings;
pub(crate) mod combat_data;
pub(crate) mod combat_effect;
pub(crate) mod combat_event;
pub(crate) mod combat_events;
pub(crate) mod combat_rules;
pub(crate) mod combatant;
pub(crate) mod damage;
pub(crate) mod damage_handle;
pub(crate) mod damage_kind;
pub(crate) mod damage_queue;
pub(crate) mod damage_weigher;
pub(crate) mod dead;
pub(crate) mod deaths;
pub(crate) mod launches;
pub(crate) mod on_death;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod targets;

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub(crate) const ROLL_STREAM: &str = "combat.roll";

/// The `combat` capability: teams, the life pool, attacks, damage and deaths.
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
    /// In `SimSet::Hit`: windups that end strike, or fire, each with its roll drawn.
    Strike,
    /// In `SimSet::Hit`, after `Strike`: the tick's launches take off.
    Launch,
    /// In `SimSet::Hit`, after `Launch`: modifiers' intervals come.
    Interval,
    /// In `SimSet::Resolve`: the tick's damage is dealt.
    Damage,
    /// In `SimSet::Resolve`, after `Damage`: units at zero life die.
    Die,
}

impl Combat {
    /// Adds combat to a match: in Inputs, dead units whose respawn is due come back; in Act,
    /// attacks in range start once ready; in Hit, windups that end strike, or fire when ranged
    /// and the match has projectiles; in Resolve, the tick's damage is dealt, then units at zero
    /// life die, each with its killer and assisters; in Vision, the dead whose type despawns
    /// go, after the Mode stage saw them.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(DamageQueue::default());
        world.insert_resource(AttackKind::default());
        world.insert_resource(CombatBindings::default());
        world.insert_resource(Deaths::default());
        schedule.configure_sets(
            CombatSet::Launch
                .in_set(SimSet::Hit)
                .after(CombatSet::Strike),
        );
        schedule.add_systems((
            attack.in_set(SimSet::Act).in_set(CombatSet::Attack),
            (attack_events, strike)
                .chain()
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Strike),
            run_intervals
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Interval)
                .after(CombatSet::Launch),
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
        registry.register_component::<OnDeath>();
        registry.register_component::<RecentAttackers>();
        registry.register_component::<Respawn>();
    }
}

impl Combat {
    /// Binds `life` as the life pool, as a client does from the packages it holds, where no mode
    /// installs.
    pub fn bind_life(world: &mut World, life: PoolId) {
        world.resource_mut::<CombatBindings>().life = life;
    }
}

/// Fills a row of the script view with what combat holds: whether the unit lives and whether it
/// stays when dead, its attack's target and range, and who struck it recently.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.alive = !unit.contains::<Dead>();
    fill.row.stays = unit.get::<OnDeath>() == Some(&OnDeath::Stay);
    fill.row.target = unit.get::<AttackState>().and_then(|attack| attack.target());
    fill.row.attack_range = unit.get::<AttackStats>().map(|stats| stats.range());
    if let Some(recent) = unit.get::<RecentAttackers>() {
        fill.attacked(recent.iter());
    }
}

/// Drops each target that is gone, dead, no longer an enemy or not a target, which cancels its
/// windup, and starts an attack when the target is in range and the unit is ready. The range
/// counts only at the start. A unit its tags keep from attacking keeps its target, and its
/// windup starts again.
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
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, stats, mut attack, body, tags) in &mut attackers {
        let Some(target) = attack.target() else {
            continue;
        };
        if UnitTags::effects_of(tags).blocks(Block::Attack) {
            if attack.started().is_some() {
                attack.interrupt();
            }
            continue;
        }
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

/// An attack that goes off this tick: its attacker and its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct GoingOff {
    attacker: StableId,
    target: StableId,
}

/// Runs `on_attack` for each attack whose windup ends this tick, by attacker's stable id, before
/// any of them strikes or fires; not for one its attacker's tags stop, which does not strike.
fn attack_events(
    world: &mut World,
    attackers: &mut QueryState<
        (&StableId, &AttackStats, &AttackState, Option<&UnitTags>),
        Without<Dead>,
    >,
    mut going: Local<'_, Vec<GoingOff>>,
) {
    let Some(events) = world.remove_non_send::<CombatEvents>() else {
        return;
    };
    let now = world.resource::<SimTick>().start();
    going.clear();
    for (&attacker, stats, attack, tags) in attackers.iter(world) {
        if attack.windup_ended(stats.windup(), now).is_some()
            && !UnitTags::effects_of(tags).blocks(Block::Attack)
        {
            let target = attack
                .target()
                .expect("an attack in its windup has a target");
            going.push(GoingOff { attacker, target });
        }
    }
    going.sort_unstable();
    if !going.is_empty() {
        let view = world.non_send::<View>().clone();
        ScriptBatch::run(world, &view, |batch| {
            for &GoingOff { attacker, target } in &*going {
                events.hear(batch, CombatEvent::Attack { attacker, target });
            }
        });
    }
    world.insert_non_send(events);
}

/// An instance whose interval comes this tick: its carrier, its modifier and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct IntervalDue {
    carrier: StableId,
    id: ModifierId,
    source: Option<StableId>,
}

/// Counts each living carrier's intervals, and runs `on_interval` of each instance whose
/// interval comes this tick, by carrier's stable id, then modifier, then source.
fn run_intervals(
    world: &mut World,
    carriers: &mut QueryState<(&StableId, &mut Modifiers, Option<&UnitTags>), Without<Dead>>,
    mut due: Local<'_, Vec<IntervalDue>>,
) {
    let now = world.resource::<SimTick>().start();
    let granting = world
        .get_resource::<TagBook>()
        .map_or(TagSet::default(), TagBook::granting);
    due.clear();
    for (&carrier, mut modifiers, tags) in carriers.iter_mut(world) {
        let immune = tags.map_or(TagSet::default(), |tags| tags.immune);
        let push = |id, source| {
            due.push(IntervalDue {
                carrier,
                id,
                source,
            });
        };
        if modifiers.bypass_change_detection().advance_intervals(
            now,
            TagBook::effect_test(granting, immune),
            push,
        ) {
            modifiers.set_changed();
        }
    }
    if due.is_empty() {
        return;
    }
    due.sort_unstable();
    let Some(events) = world.remove_non_send::<CombatEvents>() else {
        return;
    };
    let view = world.non_send::<View>().clone();
    ScriptBatch::run(world, &view, |batch| {
        for &IntervalDue {
            carrier,
            id,
            source,
        } in &*due
        {
            events.hear(
                batch,
                CombatEvent::Interval {
                    carrier,
                    id,
                    source,
                },
            );
        }
    });
    world.insert_non_send(events);
}

/// Queues the damage of each attack whose windup ends this tick, or its launch when it is ranged
/// and the match has projectiles. Each draws its roll now, at least 0 and less than 1, which
/// `calc_damage` reads. A windup whose attacker's tags keep it from attacking is interrupted
/// instead.
fn strike(
    (tick, rng, kind): (Res<'_, SimTick>, Res<'_, SimRng>, Res<'_, AttackKind>),
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
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&source, &from, stats, mut attack, tags) in &mut attackers {
        let Some(started) = attack.windup_ended(stats.windup(), now) else {
            continue;
        };
        if UnitTags::effects_of(tags).blocks(Block::Attack) {
            attack.interrupt();
            continue;
        }
        let target = attack
            .target()
            .expect("an attack in its windup has a target");
        let amount = stats.damage();
        let roll = rng.open(ROLL_STREAM, source).fraction();
        match (stats.projectile_speed(), launches.as_deref_mut()) {
            (Some(speed), Some(launches)) => launches.0.push(Launch {
                source,
                from,
                target,
                amount,
                speed,
                roll,
            }),
            _ => queue.push(Damage {
                source: Some(source),
                target,
                amount,
                kind: kind.0,
                cause: DamageCause::Attack { roll },
                ability: None,
                depth: 0,
            }),
        }
        attack.strike(started.after(stats.period()));
    }
}

/// Deals the tick's damage in the queue's order: each through the mode's `calc_damage` when it
/// has one, with the units as the pass began, then its combat events, whose damage joins the end
/// of the queue. Damage to a unit at zero life, or to an invulnerable one, does nothing.
fn deal_damage(world: &mut World, mut assisters: Local<'_, Vec<StableId>>) {
    let now = world.resource::<SimTick>().start();
    world.resource_mut::<Deaths>().clear(now);
    world.resource_mut::<DamageQueue>().sort();
    let weigher = world.remove_non_send::<DamageWeigher>();
    let events = world.remove_non_send::<CombatEvents>();
    if weigher.is_none() && events.is_none() {
        let mut at = 0;
        while let Some(damage) = world.resource::<DamageQueue>().get(at) {
            at += 1;
            Combat::deal(world, damage, damage.amount, now);
        }
    } else {
        let view = world.non_send::<View>().clone();
        ScriptBatch::run(world, &view, |batch| {
            let mut at = 0;
            while let Some(damage) = batch.world().resource::<DamageQueue>().get(at) {
                at += 1;
                if Combat::damageable(batch.world(), damage.target).is_none() {
                    continue;
                }
                let amount = weigher.as_ref().map_or(damage.amount, |weigher| {
                    weigher.weigh(batch, damage).unwrap_or_else(|error| {
                        batch.record(Some(damage.target), Hook::CalcDamage, error);
                        damage.amount
                    })
                });
                let landed = Combat::deal(batch.world(), damage, amount, now);
                if let Some(events) = &events {
                    let dealt = Damage {
                        amount: amount.max(Num::ZERO),
                        ..damage
                    };
                    Combat::answer(batch, events, dealt, landed, &mut assisters);
                }
            }
        });
    }
    if let Some(weigher) = weigher {
        world.insert_non_send(weigher);
    }
    if let Some(events) = events {
        world.insert_non_send(events);
    }
    world.resource_mut::<DamageQueue>().clear();
}

/// What a damage of the pass did: nothing, as to a unit at zero life or an invulnerable one;
/// damage; or a kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landed {
    Nothing,
    Taken,
    Killed,
}

impl Combat {
    /// The entity of `unit`, when it exists and its life pool is above zero: one that damage,
    /// heals and restores reach.
    fn living(world: &World, unit: StableId) -> Option<Entity> {
        let entity = world.resource::<EntityIndex>().get(unit)?;
        let life = world.resource::<CombatBindings>().life;
        world
            .get::<Pools>(entity)?
            .above_zero(life)
            .then_some(entity)
    }

    /// The entity of `unit`, when it is living and its tags let damage reach it.
    fn damageable(world: &World, unit: StableId) -> Option<Entity> {
        Combat::living(world, unit)
            .filter(|&entity| !UnitTags::effects_of(world.get(entity)).blocks(Block::Damage))
    }

    /// Runs the events of `damage`, which `landed`, its amount after `calc_damage`: an attack's
    /// hit, the damage taken, then a kill's: the killer's, then the takedowns of the killer and
    /// each assister, by stable id, whom `assisters` holds while their hooks run.
    fn answer(
        batch: &mut ScriptBatch<'_>,
        events: &CombatEvents,
        damage: Damage,
        landed: Landed,
        assisters: &mut Vec<StableId>,
    ) {
        if landed == Landed::Nothing {
            return;
        }
        if damage.cause.attack() {
            events.hear(batch, CombatEvent::AttackHit(damage));
        }
        events.hear(batch, CombatEvent::DamageTaken(damage));
        if landed != Landed::Killed {
            return;
        }
        let deaths = batch.world().resource::<Deaths>();
        let kill = deaths.iter().last().expect("a kill records its death");
        let killer = kill.killer;
        assisters.clear();
        assisters.extend_from_slice(kill.assisters);
        let (victim, depth) = (damage.target, damage.depth);
        if let Some(killer) = killer {
            events.hear(
                batch,
                CombatEvent::Kill {
                    killer,
                    victim,
                    depth,
                },
            );
        }
        for unit in killer.into_iter().chain(assisters.iter().copied()) {
            events.hear(
                batch,
                CombatEvent::Takedown {
                    unit,
                    victim,
                    depth,
                },
            );
        }
    }

    /// Deals `damage` as `amount` in tick `now`, a negative amount as 0: shields absorb it, then
    /// the life pool takes the rest. The source is recorded as the target's attacker; when the damage
    /// took the target to zero, the source is its killer if it still exists, and the others that
    /// damaged it within the assist window assisted. A living source heals by its `leech`
    /// stat, `attack` for an attack's damage and `other` for the rest, times the life taken.
    fn deal(world: &mut World, damage: Damage, amount: Num, now: Tick) -> Landed {
        let Some(entity) = Combat::damageable(world, damage.target) else {
            return Landed::Nothing;
        };
        let index = world.resource::<EntityIndex>();
        let source = damage.source.filter(|&source| index.get(source).is_some());
        let mut left = amount.max(Num::ZERO);
        let takes_effect = TagBook::effective(world, entity);
        if let Some(mut modifiers) = world.get_mut::<Modifiers>(entity) {
            let after = modifiers
                .bypass_change_detection()
                .absorb(left, takes_effect);
            if after != left {
                modifiers.set_changed();
            }
            left = after;
        }
        let life = world.resource::<CombatBindings>().life;
        let mut pools = world
            .get_mut::<Pools>(entity)
            .expect("a unit that takes damage");
        let taken = pools.take(life, left);
        let killed = !pools.above_zero(life);
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
            let bindings = world.resource::<CombatBindings>();
            let stat = if damage.cause.attack() {
                bindings.leech_attack
            } else {
                bindings.leech_other
            };
            let ratio = Combat::stat(world, source, stat);
            Combat::heal_living(world, source, scaled(taken, ratio));
        }
        if killed {
            Landed::Killed
        } else {
            Landed::Taken
        }
    }

    /// Applies `effect`, which a call queued from `source`, by `ability`, at chain depth
    /// `depth`: damage joins the queue, a heal or a restore applies at once, and an extra attack
    /// queues the source's attack damage, when it still has an attack.
    pub(crate) fn apply_effect(
        world: &mut World,
        effect: CombatEffect,
        source: Option<StableId>,
        ability: Option<AbilityId>,
        depth: u8,
    ) {
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            ability,
            depth,
        };
        match effect {
            CombatEffect::Damage {
                target,
                amount,
                kind,
            } => {
                let damage = damage(target, amount, kind, DamageCause::Effect);
                world.resource_mut::<DamageQueue>().push(damage);
            }
            CombatEffect::Heal { unit, amount } => Combat::heal(world, unit, amount),
            CombatEffect::Restore { unit, pool, amount } => {
                Combat::restore(world, unit, pool, amount);
            }
            CombatEffect::AttackHit { target } => {
                let index = world.resource::<EntityIndex>();
                let entity = source.and_then(|source| index.get(source));
                let Some(stats) = entity.and_then(|entity| world.get::<AttackStats>(entity)) else {
                    return;
                };
                let kind = world.resource::<AttackKind>().0;
                let hit = damage(target, stats.damage(), kind, DamageCause::ExtraAttack);
                world.resource_mut::<DamageQueue>().push(hit);
            }
        }
    }

    /// Heals `unit`'s life pool by `amount` times one plus its `heal_scale` stat, when it exists
    /// and its life is above zero.
    pub(crate) fn heal(world: &mut World, unit: StableId, amount: Num) {
        if let Some(entity) = Combat::living(world, unit) {
            Combat::heal_living(world, entity, amount);
        }
    }

    /// Heals `entity`, a living unit, as `heal` does.
    fn heal_living(world: &mut World, entity: Entity, amount: Num) {
        let bindings = *world.resource::<CombatBindings>();
        let received = Num::ONE + Combat::stat(world, entity, bindings.heal_scale);
        let amount = scaled(amount, received);
        if amount > Num::ZERO {
            let mut pools = world.get_mut::<Pools>(entity).expect("a living unit");
            pools.add(bindings.life, amount);
        }
    }

    /// Restores `amount` of `unit`'s `pool`, unscaled, when the unit exists, its life is above
    /// zero, and it has the pool.
    pub(crate) fn restore(world: &mut World, unit: StableId, pool: PoolId, amount: Num) {
        let Some(entity) = Combat::living(world, unit) else {
            return;
        };
        if amount > Num::ZERO {
            let mut pools = world.get_mut::<Pools>(entity).expect("a living unit");
            pools.add(pool, amount);
        }
    }

    /// `entity`'s value of the stat at `stat` among the stats; 0 when `[combat]` binds none, or
    /// the unit has no stats.
    fn stat(world: &World, entity: Entity, stat: Option<u16>) -> Num {
        let values = world.get::<UnitStats>(entity);
        stat.zip(values)
            .map_or(Num::ZERO, |(at, values)| values.values()[usize::from(at)])
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

/// A unit at zero life dies, and the Mode stage learns of it; one no strike took there died
/// with no killer.
fn die(
    mut commands: Commands<'_, '_>,
    bindings: Res<'_, CombatBindings>,
    mut deaths: ResMut<'_, Deaths>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Pools,
            Option<&mut AttackState>,
            Option<&Team>,
            Option<&Owner>,
        ),
        (With<OnDeath>, Without<Dead>),
    >,
) {
    for (entity, &id, pools, attack, team, owner) in &mut units {
        if pools.above_zero(bindings.life) {
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

/// Brings back each dead unit whose respawn is due, at its spawn point with full pools and no
/// one on record as its attacker.
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
            &mut Pools,
            &mut Position,
            Option<&mut RecentAttackers>,
        ),
        With<Dead>,
    >,
) {
    let now = tick.start();
    for (entity, respawn, spawn, mut pools, mut position, attackers) in &mut dead {
        if respawn.at > now {
            continue;
        }
        pools.fill();
        *position = spawn.get();
        if let Some(mut attackers) = attackers {
            *attackers = RecentAttackers::default();
        }
        commands.entity(entity).remove::<(Dead, Respawn)>();
    }
}

#[cfg(test)]
mod tests;
