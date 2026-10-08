use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_common::Tick;
use campfire_sim::{Keyed, Ordered, Position, SimRng, SimTick, StableId, TickRate};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::in_progress::InProgress;
use crate::actions::payer::Payer;
use crate::actions::purse::Purse;
use crate::actions::slot_aim::SlotAim;
use crate::actions::targets::Targets;
use crate::combat::combat_event::{CombatEvent, CombatEvents};
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::going_off::GoingOff;
use crate::combat::pass_queue::PassQueue;
use crate::combat::shots::{Shot, Shots};
use crate::combat::wielded::Wielded;
use crate::combat::{Combat, ROLL_STREAM};
use crate::players::player_resources::PlayerResources;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::pools::Pools;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::ForcedMove;
use crate::units::owner::Owner;
use crate::units::predicting::Predicting;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::view::View;

/// What tells whether a unit's attack strikes: its id, its slots, its tags, and what it pays the
/// cost from.
pub(super) type Attacker<'a> = (
    &'a StableId,
    &'a ActionSlots,
    Option<&'a UnitTags>,
    Has<ForcedMove>,
    Option<&'a Pools>,
    Option<&'a Owner>,
);

/// Attacks on the units' attack targets: their start in Act, then in Hit their toggles' pay, their events and their strikes.
#[derive(Debug)]
pub(super) struct Attacks;

impl Attacks {
    /// Starts each unit's attack on its attack target, in Act, when it has nothing under way: with
    /// the first weapon whose filter selects the target, once that weapon passes its checks and the
    /// target is within its range. An attack in its windup stops when the unit's tags keep it from
    /// attacking. An attack target that is no living enemy any more is dropped, with the attack on it.
    pub(super) fn start_attacks(
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
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
    ) {
        let now = tick.start();
        for (&position, &team, mut slots, pools, owner, body, tags, forced) in &mut units {
            let blocked = UnitTags::blocks(tags, forced, Block::Attack);
            match slots.in_progress() {
                Some(
                    InProgress::Order { .. }
                    | InProgress::Charge { .. }
                    | InProgress::Channel { .. },
                ) => {}
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
                    let selects = (targets.relation(team, unit.team), unit.tags);
                    let Some(slot) = book.weapon_for(&slots, Some(selects)) else {
                        continue;
                    };
                    let aim = SlotAim {
                        slot,
                        target: ActionTarget::Unit(target),
                    };
                    let purse = Purse::of(pools, resources.as_deref(), owner);
                    let relation = |other| targets.relation(team, other);
                    let shape = Body::shape_of(body);
                    let started = book
                        .check(now, &slots, purse, aim, relation, |id| targets.living(id))
                        .filter(|checked| checked.in_range(position, shape, &targets))
                        .map(|checked| now.after(checked.values.windup));
                    if let Some(resolves_at) = started {
                        slots.start_attack(slot, resolves_at);
                    }
                }
            }
        }
    }

    /// Runs `on_attack` for each attack whose windup ends this tick, by attacker's stable id, before
    /// any of them strikes or fires; not for one that does not strike.
    pub(super) fn attack_events(
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
        for (&attacker, slots, tags, forced, pools, owner) in attackers.iter(world) {
            let purse = Purse::of(pools, resources, owner);
            if let Some(target) = Combat::going_off(slots, now)
                && Combat::wielded(book, slots).strikes(tags, forced, purse)
            {
                going.push(GoingOff { attacker, target });
            }
        }
        going.sort_unstable();
        if !going.is_empty() {
            let view = world.non_send::<View>().clone();
            ScriptBatch::run(world, &view, |batch| {
                for &GoingOff { attacker, target } in &*going {
                    events.call(batch, CombatEvent::Attack { attacker, target });
                }
            });
        }
        world.insert_non_send(events);
    }

    /// Pays, for each attack going off this tick, its attacker's toggles that cost an attack; one its
    /// pools cannot pay turns off, before the attack's events. A client that predicts the attack pays
    /// nothing, as its pools come from the server.
    pub(super) fn pay_attack_toggles(
        (tick, book, predicting): (
            Res<'_, SimTick>,
            Res<'_, ActionBook>,
            Option<Res<'_, Predicting>>,
        ),
        mut attackers: Query<'_, '_, (&mut ActionSlots, &mut Pools), Without<Dead>>,
    ) {
        if predicting.is_some() {
            return;
        }
        let now = tick.start();
        for (mut slots, mut pools) in &mut attackers {
            if Combat::going_off(&slots, now).is_some() {
                slots.pay_attack_toggles(&book, &mut pools);
            }
        }
    }

    /// Delivers each attack whose windup ends this tick, in the order of its attacker's stable id: it
    /// queues the damage of its weapon's damage stat, of its kind, or a shot when the weapon fires a
    /// projectile, which `projectiles` launches, as the load gives such a weapon only to a match with
    /// projectiles. Each draws its roll now, at least 0 and less than 1, which `calc_damage` reads.
    /// The weapon's cost is paid, in pools and its player's resources, and it is ready again a period
    /// from the attack's start, the tick rate over its rate stat. A windup whose attacker's tags keep
    /// it from attacking, or that no longer affords its cost, stops instead, and spends nothing. A
    /// client that predicts the attack only makes the weapon ready again, as the damage, the launch
    /// and the cost come from the server.
    pub(super) fn strike(
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
                Has<ForcedMove>,
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
            let (_, &source, &from, mut slots, stats, pools, tags, forced, owner) =
                attackers.get_mut(entity).expect("an attacker in the order");
            let target = Combat::going_off(&slots, now).expect("an attack going off");
            let purse = Purse::of(pools.as_deref(), resources.as_deref(), owner);
            let wielded = Combat::wielded(&book, &slots);
            if !wielded.strikes(tags, forced, purse) {
                slots.interrupt();
                continue;
            }
            let Wielded {
                action,
                rank,
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
                    rank,
                    amount,
                    kind: weapon.kind,
                    roll,
                }),
                None => queue.push_damage(Damage {
                    source: Some(source),
                    target,
                    amount,
                    kind: weapon.kind,
                    cause: DamageCause::Attack { roll, rank },
                    ability: Some(action),
                    depth: 0,
                    hit: None,
                }),
            }
            let payer = Payer::of(pools.map(Mut::into_inner), resources.as_deref_mut(), owner);
            payer.pay(&values.cost, resource_cost);
        }
    }
}
