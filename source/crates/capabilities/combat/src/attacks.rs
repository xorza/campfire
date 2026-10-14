use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionCheck, ActionSlots, ActionTarget, InProgress, Payer, Purse, Shot, ShotHits,
    Shots, SlotAim, SlotTimes, Targets,
};
use campfire_common::Tick;
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut, PlayersWorldCrate};
use campfire_script::ScriptsWorldCrate;
use campfire_sim::{Keyed, Ordered, Position, SimRng, SimTick, StableId, TickClock};
use campfire_stats::{PoolBook, StatsWorld, UnitPools, UnitPoolsReadOnly, UnitStats};
use campfire_units::{Body, Dead, ForcedMove, Owner, Predicting};
use campfire_values::{Block, Team, UnitTags};

use crate::ROLL_STREAM;
use crate::combat_event::CombatEvent;
use crate::combat_hooks::CombatHooks;
use crate::damage::{Damage, DamageCause};
use crate::going_off::GoingOff;
use crate::pass_queue::PassQueue;
use crate::wielded::Wielded;

/// What tells whether a unit's attack strikes: its id, its slots, its tags, and what it pays the
/// cost from.
type Attacker<'a> = (
    &'a StableId,
    &'a ActionSlots,
    Option<&'a UnitTags>,
    Has<ForcedMove>,
    UnitPoolsReadOnly,
    Option<&'a Owner>,
);

/// Attacks on the units' attack targets: their start in Act, then in Hit their toggles' pay, their
/// events and their strikes.
#[derive(Debug)]
pub struct Attacks;

impl Attacks {
    /// Queues the damage of each hit of an attack's projectile this tick, in the order they
    /// struck: its weapon's damage, with the roll it drew.
    pub(super) fn take_hits(mut hits: ResMut<'_, ShotHits>, mut queue: ResMut<'_, PassQueue>) {
        for hit in hits.0.drain(..) {
            queue.push_damage(Damage {
                source: Some(hit.source),
                target: hit.target,
                amount: hit.amount,
                kind: hit.kind,
                cause: DamageCause::Attack {
                    roll: hit.roll,
                    rank: hit.rank,
                },
                action: Some(hit.action),
                depth: 0,
                hit: Some(hit.hit),
            });
        }
    }

    /// Starts each unit's attack on its attack target, in Act, when it has nothing under way: with
    /// the first weapon whose filter selects the target, once that weapon passes its checks and the
    /// target is within its range. An attack in its windup stops when the unit's tags keep it from
    /// attacking. An attack target that is no living enemy any more is dropped, with the attack on
    /// it.
    pub(super) fn start_attacks(
        tick: Res<'_, SimTick>,
        book: Res<'_, ActionBook>,
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        targets: Targets<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                &Position,
                &Team,
                &mut ActionSlots,
                &SlotTimes,
                UnitPoolsReadOnly,
                Option<&Owner>,
                Option<&Body>,
                Option<&UnitTags>,
                Has<ForcedMove>,
            ),
            Without<Dead>,
        >,
    ) {
        let now = tick.start();
        let life = life.life();
        for (&position, &team, mut slots, times, pools, owner, body, tags, forced) in &mut units {
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
                    let Some(slot) = slots.weapon_in(&book, Some(selects)) else {
                        continue;
                    };
                    let aim = SlotAim {
                        slot,
                        target: ActionTarget::Unit(target),
                    };
                    let purse = Purse::of(pools.of(life), resources.of(owner));
                    let relation = |other| targets.relation(team, other);
                    let shape = Body::shape_of(body);
                    let started = times
                        .checked_action(ActionCheck {
                            book: &book,
                            now,
                            slots: &slots,
                            purse,
                            aim,
                            relation,
                            living: |id| targets.living(id),
                        })
                        .filter(|checked| checked.in_range(position, shape, &targets))
                        .map(|checked| now.after(checked.values.windup));
                    if let Some(resolves_at) = started {
                        slots.start_attack(slot, resolves_at);
                    }
                }
            }
        }
    }

    /// Runs `on_attack` for each attack whose windup ends this tick, by attacker's stable id,
    /// before any of them strikes or fires; not for one that does not strike.
    pub(super) fn attack_events(
        world: &mut World,
        attackers: &mut QueryState<Attacker<'_>, Without<Dead>>,
        mut going: Local<'_, Vec<GoingOff>>,
    ) {
        let Some(events) = world.non_send::<CombatHooks>().events.clone() else {
            return;
        };
        let now = world.resource::<SimTick>().start();
        let book = world.resource::<ActionBook>();
        let life = world.life_pool();
        going.clear();
        for (&attacker, slots, tags, forced, pools, owner) in attackers.iter(world) {
            let purse = Purse::of(pools.of(life), world.owner_resources(owner));
            if let Some(target) = Self::going_off(slots, now)
                && Self::wielded(book, slots).strikes(tags, forced, purse)
            {
                going.push(GoingOff { attacker, target });
            }
        }
        going.sort_unstable();
        if !going.is_empty() {
            world.script_batch(|batch| {
                for &GoingOff { attacker, target } in &*going {
                    events.call(batch, CombatEvent::Attack { attacker, target });
                }
            });
        }
    }

    /// Pays, for each attack going off this tick, its attacker's toggles that cost an attack; one
    /// its pools cannot pay turns off, before the attack's events. A client that predicts the
    /// attack pays nothing, as its pools come from the server.
    pub(super) fn pay_attack_toggles(
        (tick, book, life, predicting): (
            Res<'_, SimTick>,
            Res<'_, ActionBook>,
            Res<'_, PoolBook>,
            Option<Res<'_, Predicting>>,
        ),
        mut attackers: Query<'_, '_, (&ActionSlots, &mut SlotTimes, UnitPools), Without<Dead>>,
    ) {
        if predicting.is_some() {
            return;
        }
        let now = tick.start();
        let life = life.life();
        for (slots, mut times, pools) in &mut attackers {
            if Self::going_off(slots, now).is_some() {
                times.pay_attack_toggles(slots, &book, &mut pools.of(life));
            }
        }
    }

    /// Delivers each attack whose windup ends this tick, in the order of its attacker's stable id:
    /// it queues the damage of its weapon's damage stat, of its kind, or a shot when the weapon
    /// fires a projectile, which `projectiles` launches, as the load gives such a weapon only to a
    /// match with projectiles. Each draws its roll now, at least 0 and less than 1, which
    /// `calc_damage` reads. The weapon's cost is paid, in pools and its player's resources, and it
    /// is ready again a period from the attack's start, the tick rate over its rate stat. A windup
    /// whose attacker's tags keep it from attacking, or that no longer affords its cost, stops
    /// instead, and spends nothing. A client that predicts the attack only makes the weapon ready
    /// again, as the damage, the launch and the cost come from the server.
    pub fn strike(
        (clock, rng, book, life): (
            TickClock<'_>,
            Res<'_, SimRng>,
            Res<'_, ActionBook>,
            Res<'_, PoolBook>,
        ),
        (mut queue, mut fired, mut resources, predicting): (
            ResMut<'_, PassQueue>,
            ResMut<'_, Shots>,
            PlayerRowsMut<'_, '_, PlayerResources>,
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
                &mut SlotTimes,
                Option<&UnitStats>,
                UnitPools,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&Owner>,
            ),
            Without<Dead>,
        >,
        mut order: Local<'_, Ordered>,
    ) {
        let now = clock.start();
        let life = life.life();
        let going = attackers.iter().filter_map(|(entity, &id, _, slots, ..)| {
            Self::going_off(slots, now).map(|_| Keyed { id, entity })
        });
        for &Keyed { entity, .. } in order.sort(going) {
            let (_, &source, &from, mut slots, mut times, stats, pools, tags, forced, owner) =
                attackers.get_mut(entity).expect("an attacker in the order");
            let target = Self::going_off(&slots, now).expect("an attack going off");
            let pools = pools.of(life);
            let purse = Purse::of(pools.read(), resources.of(owner));
            let wielded = Self::wielded(&book, &slots);
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
            let period = weapon.period(stats, clock.rate().hz().get(), values.windup);
            times.cool_down(wielded.slot, started.after(period));
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
                    action: Some(action),
                    depth: 0,
                    hit: None,
                }),
            }
            let mut held = resources.of_mut(owner);
            let payer = Payer::of(pools, held.as_deref_mut());
            payer.pay(&values.cost, resource_cost);
        }
    }

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
        let id = slot.action.expect("an attack's slot holds its weapon");
        let action = book.get(id).expect("a slot's action is in the book");
        let rank = slot.rank.expect("an attack's weapon is learned");
        Wielded {
            slot: underway.slot(),
            action: id,
            rank,
            weapon: action
                .kind
                .weapon()
                .expect("an attack's action is a weapon"),
            values: *action.values(rank),
            resource_cost: action.resource_cost(rank),
            projectile: action.delivery.map(|delivery| delivery.unit_type),
        }
    }
}
