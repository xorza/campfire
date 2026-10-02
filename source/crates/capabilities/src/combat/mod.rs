use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::query::{QueryState, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_math::{RngStream, Tick};
use campfire_sim::{
    Keyed, Ordered, Position, SimRng, SimSet, SimTick, StableId, StateRegistry, TickRate,
};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::{ActionSlots, ActionTarget, InProgress, SlotAim};
use crate::actions::purse::{Payer, Purse};
use crate::actions::rank_values::RankValues;
use crate::actions::targets::Targets;
use crate::actions::weapon::Weapon;
use crate::actions::{Actions, ActionsSet};
use crate::combat::combat_column::CombatColumn;
use crate::combat::combat_event::CombatEvent;
use crate::combat::combat_events::CombatEvents;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_pass::DamagePass;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::kept::Kept;
use crate::combat::modifier_hooks::ModifierHooks;
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::combat::shots::{Shot, Shots};
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::scripts::ctx::Ctx;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::StatsSet;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::life_pool::LifePool;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::Modifiers;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::predicting::Predicting;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

pub(crate) mod assist_window;
pub(crate) mod combat_api;
pub(crate) mod combat_bindings;
pub(crate) mod combat_column;
pub(crate) mod combat_data;
pub(crate) mod combat_effect;
pub(crate) mod combat_event;
pub(crate) mod combat_events;
pub(crate) mod combat_rules;
pub(crate) mod damage;
pub(crate) mod damage_handle;
pub(crate) mod damage_pass;
pub(crate) mod damage_weigher;
pub(crate) mod deaths;
pub(crate) mod heal;
pub(crate) mod heal_handle;
pub(crate) mod heal_weigher;
pub(crate) mod kept;
pub(crate) mod modifier_hooks;
pub(crate) mod on_death;
pub(crate) mod pass_queue;
pub(crate) mod recent_attack;
pub(crate) mod recent_attackers;
pub(crate) mod respawn;
pub(crate) mod shots;

/// The random stream an attack's roll draws from, for its attacker in its tick.
pub(crate) const ROLL_STREAM: RngStream = RngStream::new("combat.roll");

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
    /// In `SimSet::Hit`, after `Strike`: the tick's shots become launches, before any cast
    /// delivers.
    Fire,
    /// In `SimSet::Hit`, after `Fire`: the tick's launches take off.
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
        let view = world.non_send::<View>().clone();
        view.add_column(CombatColumn::default());
        view.add_source::<RowParts>(world, fill_row);
        world.insert_resource(PassQueue::default());
        world.insert_resource(Shots::default());
        world.insert_resource(Deaths::default());
        if let Some(ctx) = world.get_non_send::<Ctx>().cloned() {
            let hooks = ModifierHooks::new(ctx);
            world.insert_non_send(CombatEvents::new(move |batch, event| {
                hooks.hear(batch, event);
            }));
        }
        schedule.configure_sets((
            CombatSet::Fire.in_set(SimSet::Hit).after(CombatSet::Strike),
            CombatSet::Launch.in_set(SimSet::Hit).after(CombatSet::Fire),
            CombatSet::Die.before(StatsSet::Hold),
            CombatSet::Attack
                .in_set(SimSet::Act)
                .before(ActionsSet::Start),
            ActionsSet::HoldAtResolve
                .after(CombatSet::Damage)
                .before(CombatSet::Die),
        ));
        Actions::schedule(schedule);
        schedule.add_systems((
            start_attacks.in_set(CombatSet::Attack),
            (attack_events, strike)
                .chain()
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Strike),
            run_intervals
                .in_set(SimSet::Hit)
                .in_set(CombatSet::Interval)
                .after(CombatSet::Launch),
            (
                DamagePass::run.in_set(CombatSet::Damage),
                die.in_set(CombatSet::Die),
            )
                .chain()
                .in_set(SimSet::Resolve),
            respawn.in_set(SimSet::Inputs).in_set(CombatSet::Respawn),
            despawn_dead.in_set(SimSet::Vision),
        ));
        registry.register_component::<Dead>();
        registry.register_component::<Kept>();
        registry.register_component::<OnDeath>();
        registry.register_component::<RecentAttackers>();
        registry.register_component::<Respawn>();
    }
}

impl Combat {
    /// The target of the attack of a unit with `slots` whose windup ends by `now`, if one does.
    fn going_off(slots: &ActionSlots, now: Tick) -> Option<StableId> {
        let target = slots.attacking()?;
        let resolves_at = slots.in_progress()?.resolves_at()?;
        (resolves_at <= now).then_some(target)
    }

    /// The weapon of the attack under way of a unit with `slots`.
    fn wielded<'a>(book: &'a ActionBook, slots: &ActionSlots) -> Wielded<'a> {
        let underway = slots.in_progress().expect("an attack is under way");
        let slot = slots.slot(underway.slot()).expect("an attack's slot");
        let action = book
            .get(slot.action)
            .expect("a slot's action is in the book");
        Wielded {
            slot: underway.slot(),
            action: slot.action,
            weapon: action
                .kind
                .weapon()
                .expect("an attack's action is a weapon"),
            values: action.values(slot.rank),
            resource_cost: action.resource_cost(slot.rank),
            projectile: action.delivery.map(|delivery| delivery.unit_type),
        }
    }

    /// The match's life pool; `None` when its mode names none.
    pub fn life(world: &World) -> Option<PoolId> {
        Some(world.get_resource::<LifePool>()?.0)
    }
}

/// The parts of a unit combat reads into its row: whether it is dead, its pools and tags, what
/// it does when it dies, and who struck it recently.
type RowParts = (
    Has<Dead>,
    Option<&'static Pools>,
    Option<&'static UnitTags>,
    Option<&'static OnDeath>,
    Option<&'static RecentAttackers>,
);

/// Fills a row of the script view with what combat holds: whether the unit lives and whether it
/// may be a target, in the core's row; whether it stays when dead, and who struck it recently, in
/// combat's column.
fn fill_row(parts: ROQueryItem<'_, '_, RowParts>, fill: &mut RowFill<'_>) {
    let (dead, pools, tags, on_death, recent) = parts;
    let alive = !dead;
    fill.row.alive = alive;
    fill.row.targetable = alive
        && fill
            .world
            .get_resource::<LifePool>()
            .is_some_and(|life| Targets::targetable(pools, tags, life.0));
    let stays = on_death == Some(&OnDeath::Stay);
    fill.column::<CombatColumn>()
        .push(stays, recent.into_iter().flat_map(RecentAttackers::iter));
}

/// Starts each unit's attack on its attack target, in Act, when it has nothing under way: with
/// the first weapon whose filter selects the target, once that weapon passes its checks and the
/// target is within its range. An attack in its windup stops when the unit's tags keep it from
/// attacking. An attack target that is no living enemy any more is dropped, with the attack on it.
fn start_attacks(
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
        let blocked = UnitTags::effects_of(tags).blocks(Block::Attack);
        match slots.in_progress() {
            Some(InProgress::Order { .. }) => {}
            Some(InProgress::Attack { .. }) => {
                if blocked {
                    slots.interrupt();
                } else if slots
                    .attack_target()
                    .is_none_or(|target| targets.enemy(team, target).is_none())
                {
                    slots.set_attack_target(None);
                }
            }
            None => {
                let Some(target) = slots.attack_target() else {
                    continue;
                };
                let Some(unit) = targets.enemy(team, target) else {
                    slots.set_attack_target(None);
                    continue;
                };
                if blocked {
                    continue;
                }
                let selects = (targets.attitude(team, unit.team), unit.tags);
                let Some(slot) = book.weapon_for(&slots, Some(selects)) else {
                    continue;
                };
                let aim = SlotAim {
                    slot,
                    target: ActionTarget::Unit(target),
                };
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
                    .map(|checked| now.after(checked.values.windup));
                if let Some(resolves_at) = started {
                    slots.start_attack(slot, resolves_at);
                }
            }
        }
    }
}

/// What tells whether a unit's attack strikes: its id, its slots, its tags, and what it pays the
/// cost from.
type Attacker<'a> = (
    &'a StableId,
    &'a ActionSlots,
    Option<&'a UnitTags>,
    Option<&'a Pools>,
    Option<&'a Owner>,
);

/// The weapon of an attack under way: its slot, what it deals, and its values and its cost in
/// player resources at the slot's rank.
#[derive(Debug, Clone, Copy)]
struct Wielded<'a> {
    slot: u8,
    /// The weapon's action, which its damage names.
    action: ActionId,
    weapon: Weapon,
    values: RankValues,
    resource_cost: &'a [ResourceAmount],
    /// The type of the homing projectile it fires, if it fires one.
    projectile: Option<UnitType>,
}

impl Wielded<'_> {
    /// Whether its attack, going off, strikes for a unit with `tags`: no tag keeps it from
    /// attacking, and `purse` still affords its cost, as the checks run again at delivery.
    fn strikes(&self, tags: Option<&UnitTags>, purse: Purse<'_>) -> bool {
        !UnitTags::effects_of(tags).blocks(Block::Attack)
            && purse.affords(&self.values.cost, self.resource_cost)
    }
}

/// An attack that goes off this tick: its attacker and its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct GoingOff {
    attacker: StableId,
    target: StableId,
}

/// Runs `on_attack` for each attack whose windup ends this tick, by attacker's stable id, before
/// any of them strikes or fires; not for one that does not strike.
fn attack_events(
    world: &mut World,
    attackers: &mut QueryState<Attacker<'_>, Without<Dead>>,
    mut going: Local<'_, Vec<GoingOff>>,
) {
    let Some(events) = world.remove_non_send::<CombatEvents>() else {
        return;
    };
    let now = world.resource::<SimTick>().start();
    let book = world.resource::<ActionBook>();
    let resources = world.get_resource::<PlayerResources>();
    going.clear();
    for (&attacker, slots, tags, pools, owner) in attackers.iter(world) {
        let purse = Purse {
            pools,
            resources,
            owner: owner.map(|owner| owner.slot()),
        };
        if let Some(target) = Combat::going_off(slots, now)
            && Combat::wielded(book, slots).strikes(tags, purse)
        {
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
    carriers: &mut QueryState<
        (
            &StableId,
            &mut Modifiers,
            &mut ModifierClocks,
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
    mut due: Local<'_, Vec<IntervalDue>>,
) {
    let now = world.resource::<SimTick>().start();
    let granting = world
        .get_resource::<TagBook>()
        .map_or(TagSet::default(), TagBook::granting);
    let Some(book) = world.get_resource::<ModifierBook>().cloned() else {
        return;
    };
    due.clear();
    for (&carrier, modifiers, clocks, tags) in carriers.iter_mut(world) {
        let immune = tags.map_or(TagSet::default(), |tags| tags.immune);
        let takes_effect = TagBook::effect_test(granting, immune);
        let push = |id, source| {
            due.push(IntervalDue {
                carrier,
                id,
                source,
            });
        };
        CarriedMut::new(modifiers, clocks).advance_intervals(
            now,
            |id| takes_effect(book.tags(id)),
            push,
        );
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

/// Delivers each attack whose windup ends this tick, in the order of its attacker's stable id: it
/// queues the damage of its weapon's damage stat, of its kind, or a shot when the weapon fires a
/// projectile, which `projectiles` launches, as the load gives such a weapon only to a match with
/// projectiles. Each draws its roll now, at least 0 and less than 1,
/// which `calc_damage` reads. The weapon's cost is paid, in pools and its player's resources, and
/// it is ready again a period from the attack's start, the tick rate over its rate stat. A windup
/// whose attacker's tags keep it from attacking, or that no longer affords its cost, stops instead,
/// and spends nothing. A client that predicts the attack only makes the weapon ready again, as
/// the damage, the launch and the cost come from the server.
fn strike(
    (tick, rate, rng, book): (
        Res<'_, SimTick>,
        Res<'_, TickRate>,
        Res<'_, SimRng>,
        Res<'_, ActionBook>,
    ),
    (mut queue, mut fired, mut resources, predicting): (
        ResMut<'_, PassQueue>,
        ResMut<'_, Shots>,
        Option<ResMut<'_, PlayerResources>>,
        Option<Res<'_, Predicting>>,
    ),
    mut attackers: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Position,
            &mut ActionSlots,
            Option<&UnitStats>,
            Option<&mut Pools>,
            Option<&UnitTags>,
            Option<&Owner>,
        ),
        Without<Dead>,
    >,
    mut order: Local<'_, Ordered>,
) {
    let now = tick.start();
    let going = attackers.iter().filter_map(|(entity, &id, _, slots, ..)| {
        Combat::going_off(slots, now).map(|_| Keyed { id, entity })
    });
    for &Keyed { entity, .. } in order.sort(going) {
        let (_, &source, &from, mut slots, stats, pools, tags, owner) =
            attackers.get_mut(entity).expect("an attacker in the order");
        let target = Combat::going_off(&slots, now).expect("an attack going off");
        let owner = owner.map(|owner| owner.slot());
        let purse = Purse {
            pools: pools.as_deref(),
            resources: resources.as_deref(),
            owner,
        };
        let wielded = Combat::wielded(&book, &slots);
        if !wielded.strikes(tags, purse) {
            slots.interrupt();
            continue;
        }
        let Wielded {
            action,
            weapon,
            values,
            resource_cost,
            ..
        } = wielded;
        let stats = stats.map_or(&[][..], UnitStats::values);
        let resolves_at = slots
            .in_progress()
            .and_then(InProgress::resolves_at)
            .expect("an attack going off started");
        let started = Tick::new(resolves_at.get() - values.windup.get());
        let period = weapon.period(stats, rate.hz().get(), values.windup);
        slots.cool_down(wielded.slot, started.after(period));
        slots.stop();
        if predicting.is_some() {
            continue;
        }
        let amount = weapon.damage(stats);
        let roll = rng.open(ROLL_STREAM, source).fraction();
        match wielded.projectile {
            Some(unit_type) => fired.0.push(Shot {
                source,
                from,
                target,
                unit_type,
                action,
                amount,
                kind: weapon.kind,
                roll,
            }),
            None => queue.push_damage(Damage {
                source: Some(source),
                target,
                amount,
                kind: weapon.kind,
                cause: DamageCause::Attack { roll },
                ability: Some(action),
                depth: 0,
                hit: None,
            }),
        }
        let payer = Payer {
            pools: pools.map(Mut::into_inner),
            resources: resources.as_deref_mut(),
            owner,
        };
        payer.pay(&values.cost, resource_cost);
    }
}

/// A unit at zero life dies: its attack target, and the action it ordered or has under way, end,
/// so nothing it began goes on after a respawn, and the Mode stage learns of it; one no strike
/// took there died with no killer.
fn die(
    mut commands: Commands<'_, '_>,
    life: Res<'_, LifePool>,
    mut deaths: ResMut<'_, Deaths>,
    mut units: Query<
        '_,
        '_,
        (
            Entity,
            &StableId,
            &Pools,
            Option<&mut ActionSlots>,
            Option<&Team>,
            Option<&Owner>,
        ),
        (With<OnDeath>, Without<Dead>),
    >,
) {
    for (entity, &id, pools, slots, team, owner) in &mut units {
        if pools.above_zero(life.0) {
            continue;
        }
        if let Some(mut slots) = slots {
            slots.set_attack_target(None);
            slots.stop();
        }
        commands.entity(entity).insert(Dead);
        if !deaths.contains(id) {
            deaths.push(Fallen::of(id, team, owner), None, []);
        }
    }
}

/// Despawns the dead whose unit type despawns, at the end of the tick they died in, or of the tick
/// a later stage stopped keeping them.
fn despawn_dead(
    mut commands: Commands<'_, '_>,
    dead: Query<'_, '_, (Entity, &OnDeath), (With<Dead>, Without<Kept>)>,
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
            attackers.clear();
        }
        commands.entity(entity).remove::<(Dead, Respawn)>();
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_math::{Num, Ticks};
    use campfire_sim::TickRate;

    #[cfg(test)]
    use crate::combat::on_death::OnDeath;
    #[cfg(test)]
    use crate::combat::recent_attackers::RecentAttackers;
    #[cfg(test)]
    use crate::stats::pools::Pools;
    #[cfg(test)]
    use crate::units::team::Team;

    use crate::actions::action_book::internals::{self, TestWeapon};
    use crate::actions::action_data::Range;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::slot_kind::SlotKind;
    #[cfg(test)]
    use crate::combat::combat_bindings::CombatBindings;
    #[cfg(test)]
    use crate::stats::life_pool::LifePool;

    #[cfg(test)]
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_book::StatBook;
    use crate::stats::unit_stats::UnitStats;
    use crate::units::filter::Filter;
    use crate::units::unit_type::UnitType;
    use crate::values::relation::Relation;

    /// A test unit's weapon: it aims at enemies within `range`, winds up `windup`, may attack
    /// again `period` after an attack's start, deals `damage`, and fires a projectile of the
    /// homing type `projectile` when given.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Arms {
        range: Num,
        windup: Ticks,
        period: Ticks,
        damage: Num,
        projectile: Option<UnitType>,
    }

    /// A unit's slots and stats: its weapon's slot, if it has one, and the stats it reads.
    #[derive(Debug, Bundle)]
    pub struct ArmsParts {
        slots: ActionSlots,
        stats: UnitStats,
    }

    impl ArmsParts {
        /// The parts of a unit with no weapon: no slot, and no stats.
        pub fn unarmed() -> ArmsParts {
            ArmsParts {
                slots: ActionSlots::new([]),
                stats: UnitStats::default(),
            }
        }
    }

    impl Arms {
        /// A melee weapon of `range`, `windup` ticks, `period` ticks and `damage`.
        pub const fn melee(range: Num, windup: u64, period: u64, damage: Num) -> Arms {
            Arms {
                range,
                windup: Ticks::new(windup),
                period: Ticks::new(period),
                damage,
                projectile: None,
            }
        }

        /// The same weapon, firing projectiles of the homing type `projectile`.
        #[must_use]
        pub const fn ranged(self, projectile: UnitType) -> Arms {
            Arms {
                projectile: Some(projectile),
                ..self
            }
        }

        /// The weapon added to the book of `world`, in a unit's one slot, and the stats it reads:
        /// the rate that makes its period at the match's rate, then its damage, then 0 for every
        /// other stat of the match's stat book.
        pub fn parts(self, world: &mut World) -> ArmsParts {
            let hz = world.resource::<TickRate>().hz().get();
            let weapon = TestWeapon {
                projectile: self.projectile,
                ..TestWeapon::new(
                    Filter::of_relation(Relation::Enemies),
                    Range::Meters(self.range),
                    self.windup,
                )
            };
            let id = internals::weapon(world, weapon);
            // The rate whose attacks are `period` ticks apart, rounded up so the period
            // rounds back to `period`.
            let bits = (u128::from(hz) << (2 * Num::FRAC_BITS))
                .div_ceil(u128::from(self.period.get()) << Num::FRAC_BITS);
            let rate = Num::from_bits(i64::try_from(bits).unwrap());
            // One value for each stat of the match's book, as a refresh gives a unit, the rest 0.
            let count = world
                .get_resource::<StatBook>()
                .map_or(2, |book| usize::from(book.len()));
            let mut stats = UnitStats::default();
            let values = stats.refill();
            values.extend([rate, self.damage]);
            values.resize(count.max(2), Num::ZERO);
            ArmsParts {
                slots: ActionSlots::new([(id, SlotKind::new(0), 1)]),
                stats,
            }
        }
    }

    /// Binds `life` as the life pool, with no stat bound, as a test world with no mode needs.
    #[cfg(test)]
    pub(crate) fn bind_life(world: &mut World, life: PoolId) {
        world.insert_resource(LifePool(life));
        world.insert_resource(CombatBindings::UNBOUND);
    }

    /// A test unit's combat values: the life pool it starts with, whether it stays when it dies,
    /// and its one weapon, if it has one.
    #[cfg(test)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Armed {
        pub(crate) life: Num,
        pub(crate) on_death: OnDeath,
        pub(crate) arms: Option<Arms>,
    }

    #[cfg(test)]
    impl Armed {
        /// A unit of `life` that despawns when it dies, with a melee weapon of `range`, `windup`
        /// ticks, `period` ticks and `damage`.
        pub(crate) fn melee(life: Num, range: Num, windup: u64, period: u64, damage: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: Some(Arms::melee(range, windup, period, damage)),
            }
        }

        /// A unit of `life` with no weapon, that despawns when it dies.
        pub(crate) const fn unarmed(life: Num) -> Armed {
            Armed {
                life,
                on_death: OnDeath::Despawn,
                arms: None,
            }
        }

        /// The same unit, which `on_death` says whether it stays when it dies.
        pub(crate) const fn on_death(self, on_death: OnDeath) -> Armed {
            Armed { on_death, ..self }
        }

        /// The same unit, its weapon firing projectiles of the homing type `projectile`.
        pub(crate) fn ranged(self, projectile: UnitType) -> Armed {
            let arms = self
                .arms
                .expect("a ranged unit is armed")
                .ranged(projectile);
            Armed {
                arms: Some(arms),
                ..self
            }
        }

        /// The components of a new unit of this type on `team` in `world`, whose book takes its
        /// weapon: its team and its life pool, which the spawn and its kit give; its death and
        /// its attackers, which its `combat` gives; and its weapon's parts.
        pub(crate) fn bundle(self, world: &mut World, team: Team) -> impl Bundle + use<> {
            let parts = self
                .arms
                .map_or_else(ArmsParts::unarmed, |arms| arms.parts(world));
            (
                team,
                Pools::life(self.life),
                self.on_death,
                RecentAttackers::default(),
                parts,
            )
        }
    }
}

#[cfg(test)]
mod tests;
