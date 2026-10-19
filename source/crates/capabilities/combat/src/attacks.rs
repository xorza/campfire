use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryEntityError, Without};
use bevy_ecs::system::{Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionCheck, ActionProgress, ActionSlots, ActionTarget, ByAction, Caster,
    InProgress, Payer, ProgressIndexes, Purse, Reached, Shot, ShotDamage, Shots, SlotAim,
    SlotCommit, SlotTimes, StartFrom, Targets,
};
use campfire_common::Tick;
use campfire_declare::{ByType, RoleForm};
use campfire_deliveries::{AttackShot, Delivered, DeliveredPayload, DeliveryQueue};
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut};
use campfire_script::ScriptsWorld;
use campfire_sim::{DueUnit, EntityIndex, Position, SimRng, SimTick, StableId, TickClock};
use campfire_stats::{ModifierEvents, PoolBook, UnitPools, UnitPoolsReadOnly, UnitStats};
use campfire_units::{Body, Dead, Due, Facing, ForcedMove, MapBook, Owner, UnitsNear};
use campfire_values::{Block, Motion, TagSet, Team, UnitTags, UnitType};

use crate::clip_shots::ClipShots;
use crate::combat_event::CombatEvent;
use crate::damage::{Damage, DamageCause};
use crate::held_hits::HeldHits;
use crate::pass_queue::PassQueue;
use crate::turret::Turret;
use crate::turret::turret_angle::TurretAngle;
use crate::volley::{GoingOff, Volley};
use crate::weapon::Weapon;
use crate::wielded::Wielded;
use crate::{ROLL_STREAM, SCATTER_STREAM};

/// What tells whether a unit's attack strikes: its slots and what it has under way, its tags,
/// and what it pays the cost from.
type Attacker<'a> = (
    (&'a ActionSlots, &'a ActionProgress),
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
    /// Queues the damage of each slow weapon's hit that lands this tick, in the order shot, on
    /// its target when it still lives, whatever its tags let it be as a target by then
    /// (04-capabilities/combat.md, A slow weapon delays its damage).
    pub(super) fn land_held(
        (tick, targets): (Res<'_, SimTick>, Targets<'_, '_>),
        (mut held, mut queue): (ResMut<'_, HeldHits>, ResMut<'_, PassQueue>),
        mut due: Local<'_, Vec<Damage>>,
    ) {
        let now = tick.start();
        if !held.lands_by(now) {
            return;
        }
        due.clear();
        held.land(now, &mut due);
        for &damage in &*due {
            if targets.lives(damage.target) {
                queue.push_damage(damage, now);
            }
        }
    }

    /// Queues the damage of each hit and each end of an attack's projectile this tick, of the
    /// tick's deliveries, in the order they came, its weapon's damage with the roll it drew: to the
    /// unit a hit struck, for a weapon of no splash; and where a projectile of a weapon of a splash
    /// ends, to each living unit of the relations its splash affects, to its source when it spares
    /// none, whose volume lies nearer the end's point in space than its radius, by stable id
    /// (04-capabilities/combat.md, An attack may splash).
    pub(super) fn take_hits(
        (tick, weapons): (Res<'_, SimTick>, Res<'_, ByAction<Weapon>>),
        (deliveries, mut queue): (Res<'_, DeliveryQueue>, ResMut<'_, PassQueue>),
        targets: Targets<'_, '_>,
        near: UnitsNear<'_>,
        mut splashed: Local<'_, Vec<StableId>>,
    ) {
        let now = tick.start();
        for &delivered in deliveries.delivered() {
            let Delivered {
                payload:
                    DeliveredPayload::Attack(AttackShot {
                        source,
                        team,
                        damage: shot,
                    }),
                reach,
                hit,
            } = delivered
            else {
                continue;
            };
            let damage = |target| Damage {
                source: Some(source),
                target,
                amount: shot.amount,
                kind: shot.kind,
                cause: DamageCause::Attack {
                    roll: shot.roll,
                    rank: shot.rank,
                },
                action: Some(shot.action),
                depth: 0,
                hit: Some(hit),
            };
            match (
                reach,
                weapons.get(shot.action).and_then(|weapon| weapon.splash),
            ) {
                (Reached::Hit(target), None) => queue.push_damage(damage(target), now),
                (Reached::End, Some(splash)) => {
                    let point = targets.map().point(hit.pos);
                    splashed.clear();
                    near.current().visit_near(point, splash.radius, |body| {
                        let Some(unit) = targets.living_at(body.key) else {
                            return ControlFlow::Continue(());
                        };
                        let affects = if unit.id == source {
                            !splash.spares_source
                        } else {
                            splash.affects.selects(targets.relation(team, unit.team))
                        };
                        let within = targets
                            .volume_at(body.key)
                            .is_some_and(|volume| volume.nearest(point, splash.radius).is_lt());
                        if affects && within {
                            splashed.push(unit.id);
                        }
                        ControlFlow::Continue(())
                    });
                    splashed.sort_unstable();
                    for &target in &*splashed {
                        queue.push_damage(damage(target), now);
                    }
                }
                (Reached::Hit(_), Some(_)) | (Reached::End, None) => {}
            }
        }
    }

    /// Starts the attack of each unit the index of the attack targets holds on its attack target,
    /// in Act, when it has nothing under way, and forgets each gone or with no target: with
    /// the first weapon whose filter selects the target, once that weapon passes its checks, the
    /// target is within its reach, and, for a weapon of its turret's slot kind, the turret lies
    /// within its tolerance of the target's bearing (04-capabilities/combat.md, A unit type may
    /// have a turret). An attack in its windup stops when the unit's tags keep it from
    /// attacking, or it lives no more, and such a unit starts none. An attack target that is no
    /// living enemy any more is dropped, with the attack on it.
    pub(super) fn start_attacks(
        (tick, (book, weapons), turrets): (
            Res<'_, SimTick>,
            (Res<'_, ActionBook>, Res<'_, ByAction<Weapon>>),
            Res<'_, ByType<Turret>>,
        ),
        (resources, life): (PlayerRows<'_, '_, PlayerResources>, Res<'_, PoolBook>),
        targets: Targets<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                (Entity, &StableId, &Position),
                &Team,
                (&ActionSlots, &mut ActionProgress),
                &SlotTimes,
                UnitPoolsReadOnly,
                Option<&Owner>,
                Option<&Body>,
                Option<&UnitTags>,
                Has<ForcedMove>,
                (&UnitType, Option<&Facing>, Option<&TurretAngle>),
            ),
            Without<Dead>,
        >,
        (mut indexes, mut held): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
    ) {
        let now = tick.start();
        let life = life.life();
        held.clear();
        held.extend(indexes.attackers());
        for &DueUnit { id, entity } in &*held {
            let Ok((
                (_, _, &position),
                &team,
                (slots, mut progress),
                times,
                pools,
                owner,
                body,
                tags,
                forced,
                (&unit_type, facing, turret_angle),
            )) = units.get_mut(entity).map_err(|error| {
                if matches!(error, QueryEntityError::NotSpawned(_)) {
                    indexes.forget_attacker(id);
                }
            })
            else {
                continue;
            };
            if progress.attack_target().is_none() {
                indexes.forget_attacker(id);
                continue;
            }
            let blocked =
                Caster::of(tags, Motion::of(forced), None).blocks(Block::Attack) || !pools.lives();
            match progress.in_progress() {
                Some(
                    InProgress::Order { .. }
                    | InProgress::Charge { .. }
                    | InProgress::Channel { .. },
                ) => {}
                Some(InProgress::Attack { .. }) => {
                    if blocked {
                        progress.interrupt(&mut indexes.of(id, entity));
                    } else if progress
                        .attack_target()
                        .is_none_or(|target| targets.enemy(team, target).is_none())
                    {
                        progress.set_attack_target(None, &mut indexes.of(id, entity));
                    }
                }
                None => {
                    let Some(target) = progress.attack_target() else {
                        continue;
                    };
                    let Some(unit) = targets.enemy(team, target) else {
                        progress.set_attack_target(None, &mut indexes.of(id, entity));
                        continue;
                    };
                    if blocked {
                        continue;
                    }
                    let selects = (targets.relation(team, unit.team), unit.tags);
                    let Some(slot) = slots.weapon_in(&book, Some(selects)) else {
                        continue;
                    };
                    let kind = slots.slot(slot).expect("a weapon's slot").kind;
                    let turret = turrets.get(unit_type).filter(|turret| turret.slot == kind);
                    if let (Some(turret), Some(&facing), Some(&angle)) =
                        (turret, facing, turret_angle)
                    {
                        let aim = TurretAngle::toward(facing, position.get(), unit.pos);
                        if !aim.is_some_and(|aim| angle.within(aim, turret.tolerance)) {
                            continue;
                        }
                    }
                    let aim = SlotAim {
                        slot,
                        target: ActionTarget::Unit(target),
                    };
                    let purse = Purse::of(pools.of(life), resources.of(owner));
                    let relation = |other| targets.relation(team, other);
                    let from = StartFrom {
                        pos: position.get(),
                        body,
                        targets: &targets,
                    };
                    let check = ActionCheck {
                        book: &book,
                        now,
                        caster: Caster::of(tags, Motion::of(forced), None),
                        slots,
                        purse,
                        aim,
                        relation,
                        living: |id| targets.living(id),
                    };
                    let pitch = slots.slot(slot).and_then(|held| held.action);
                    let pitch = pitch.and_then(|action| weapons.get(action)?.pitch);
                    // A weapon's bound of pitch is a bound of its reach, which its start tests
                    // past the range (04-capabilities/combat.md, An attack may bound its pitch).
                    let pitched = pitch.is_none_or(|pitch| {
                        let center = Body::center(body, position.get(), targets.map());
                        let aimed = targets.volume_of(target);
                        aimed.is_some_and(|aimed| pitch.reaches(center, &aimed))
                    });
                    let started = times
                        .start(check, Some(from))
                        .filter(|started| started.reached && pitched)
                        .map(|started| now.after(started.checked.values.windup));
                    if let Some(resolves_at) = started {
                        progress.start_attack(slot, resolves_at, &mut indexes.of(id, entity));
                    }
                }
            }
        }
    }

    /// Collects each attack of the tick's volley that strikes, by attacker's stable id, for
    /// `attack_events`, in a match that runs scripts.
    pub(super) fn collect_attacks(
        (volley, (book, weapons), pools): (
            Res<'_, Volley>,
            (Res<'_, ActionBook>, Res<'_, ByAction<Weapon>>),
            Option<Res<'_, PoolBook>>,
        ),
        attackers: Query<'_, '_, Attacker<'_>, Without<Dead>>,
        resources: PlayerRows<'_, '_, PlayerResources>,
        due: Option<ResMut<'_, Due<GoingOff>>>,
    ) {
        let Some(mut due) = due else {
            return;
        };
        let life = pools.as_deref().and_then(PoolBook::life);
        due.refill(volley.iter().filter(|off| {
            let Ok(((slots, progress), tags, forced, pools, owner)) = attackers.get(off.entity)
            else {
                return false;
            };
            let lives = pools.lives();
            let purse = Purse::of(pools.of(life), resources.of(owner));
            let wielded = Self::wielded(&book, &weapons, slots, progress);
            lives && wielded.strikes(tags, Motion::of(forced), purse)
        }));
    }

    /// Runs `on_attack` for each attack `collect_attacks` found striking, in its order, before any
    /// of them strikes or fires; not for one that does not strike.
    pub(super) fn attack_events(world: &mut World) {
        let Some(mut due) = world.get_resource_mut::<Due<GoingOff>>() else {
            return;
        };
        let going = due.take();
        if let Some(events) = ModifierEvents::of(world).filter(|_| !going.is_empty()) {
            world.script_batch(|batch| {
                for &GoingOff {
                    attacker, target, ..
                } in &going
                {
                    CombatEvent::Attack { attacker, target }.reach(&events, batch);
                }
            });
        }
        world.resource_mut::<Due<GoingOff>>().give_back(going);
    }

    /// Pays, for each attack of the tick's volley that strikes, as its events and its strike
    /// test it, its attacker's toggles that cost an attack; one its pools cannot pay and still
    /// afford the attack's own cost turns off, before the attack's events, so the attack strikes.
    /// Only the server pays them, as a client's pools come from the server.
    pub(super) fn pay_attack_toggles(
        ((book, weapons), life, volley): (
            (Res<'_, ActionBook>, Res<'_, ByAction<Weapon>>),
            Res<'_, PoolBook>,
            Res<'_, Volley>,
        ),
        resources: PlayerRows<'_, '_, PlayerResources>,
        mut attackers: Query<
            '_,
            '_,
            (
                (&ActionSlots, &ActionProgress),
                &mut SlotTimes,
                Option<&UnitTags>,
                Has<ForcedMove>,
                UnitPools,
                Option<&Owner>,
            ),
            Without<Dead>,
        >,
    ) {
        let life = life.life();
        for off in volley.iter() {
            let Ok(((slots, progress), mut times, tags, forced, pools, owner)) =
                attackers.get_mut(off.entity)
            else {
                continue;
            };
            if !pools.lives() {
                continue;
            }
            let mut pools = pools.of(life);
            let purse = Purse::of(pools.read(), resources.of(owner));
            let wielded = Self::wielded(&book, &weapons, slots, progress);
            if wielded.strikes(tags, Motion::of(forced), purse) {
                times.pay_attack_toggles(slots, &book, &mut pools, &wielded.values.cost);
            }
        }
    }

    /// Delivers each attack whose windup ends this tick, in the order of its attacker's stable id:
    /// it queues the damage of its weapon, its damage stat or its own amount, of its kind, or a
    /// shot when the weapon fires a projectile, which `projectiles` launches, as the load gives
    /// such a weapon only to a match with projectiles: at its target, or at the point of the
    /// ground its scatter draws, for a living target. Each draws its roll now, at least 0 and
    /// less than 1, which `calc_damage` reads. The weapon's cost is paid, in pools and its
    /// player's resources, and it is ready again a period from the attack's start, its own or the
    /// tick rate over its rate stat; a shot that empties its clip makes it ready its reload from
    /// the attack's start instead, its clip full again. A windup whose attacker's tags keep it
    /// from attacking, or that no longer affords its cost, stops instead, and spends nothing. A
    /// client that predicts the attack only makes the weapon ready again and counts its clip, as
    /// the damage, the launch and the cost come from the server, so it holds no held hits, which
    /// the server alone computes. An attacker that lives no more strikes nothing, as one its tags
    /// keep from attacking.
    pub(super) fn strike<F: RoleForm>(
        (clock, rng, (book, weapons), life): (
            TickClock<'_>,
            Res<'_, SimRng>,
            (Res<'_, ActionBook>, Res<'_, ByAction<Weapon>>),
            Res<'_, PoolBook>,
        ),
        (mut queue, mut fired, mut resources, mut volley): (
            ResMut<'_, PassQueue>,
            ResMut<'_, Shots>,
            PlayerRowsMut<'_, '_, PlayerResources>,
            ResMut<'_, Volley>,
        ),
        mut attackers: Query<
            '_,
            '_,
            (
                &Position,
                (&ActionSlots, &mut ActionProgress),
                &mut SlotTimes,
                Option<&UnitStats>,
                UnitPools,
                Option<&UnitTags>,
                Has<ForcedMove>,
                Option<&Owner>,
                (&mut ClipShots, Option<&Body>),
            ),
            Without<Dead>,
        >,
        (index, aims, map, mut held): (
            Res<'_, EntityIndex>,
            Query<'_, '_, (&Position, Option<&UnitTags>, Option<&Body>), Without<Dead>>,
            Res<'_, MapBook>,
            Option<ResMut<'_, HeldHits>>,
        ),
        (mut going, mut indexes): (Local<'_, Vec<GoingOff>>, ProgressIndexes<'_>),
    ) {
        let now = clock.start();
        let life = life.life();
        volley.take(&mut going);
        for &GoingOff {
            attacker: source,
            entity,
            target,
        } in &*going
        {
            let Ok((
                &from,
                (slots, mut progress),
                mut times,
                stats,
                pools,
                tags,
                forced,
                owner,
                (mut clips, body),
            )) = attackers.get_mut(entity)
            else {
                indexes.hold_resolve(source, entity);
                continue;
            };
            let lives = pools.lives();
            let pools = pools.of(life);
            let purse = Purse::of(pools.read(), resources.of(owner));
            let wielded = Self::wielded(&book, &weapons, slots, &progress);
            let dues = &mut indexes.of(source, entity);
            if !lives || !wielded.strikes(tags, Motion::of(forced), purse) {
                progress.interrupt(dues);
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
            let resolves_at = progress
                .in_progress()
                .and_then(InProgress::resolves_at)
                .expect("an attack going off started");
            let started = Tick::new(resolves_at.get() - values.windup.get());
            let period = weapon.period(stats, rank, values.windup, clock.rate().hz().get());
            let clip = weapon.clip(rank);
            let reloads = clip.is_some_and(|clip| clips.shoot(action, clip));
            let ready = match clip {
                Some(clip) if reloads => clip.reload,
                _ => period,
            };
            times.commit(SlotCommit {
                slot: wielded.slot,
                now,
                ready_at: started.after(ready),
                charges: values.charges,
                toggle: None,
            });
            progress.stop(dues);
            if F::PREDICTS {
                continue;
            }
            let amount = weapon.damage(stats, rank);
            let roll = rng.open(ROLL_STREAM, source).fraction();
            if let Some(unit_type) = wielded.projectile {
                let aimed = index.get(target).and_then(|target| aims.get(target).ok());
                let scattered = weapon
                    .scatter
                    .as_ref()
                    .zip(aimed)
                    .and_then(|(scatter, aimed)| {
                        let (pos, tags, _) = aimed;
                        let tags = tags.map_or_else(TagSet::default, |tags| tags.tags);
                        scatter.point(pos.get(), tags, || {
                            let mut draws = rng.open(SCATTER_STREAM, source);
                            [draws.fraction(), draws.fraction()]
                        })
                    });
                fired.fire(Shot {
                    source,
                    from: from.get(),
                    target: scattered.map_or(ActionTarget::Unit(target), ActionTarget::Point),
                    unit_type,
                    damage: ShotDamage {
                        action,
                        rank,
                        amount,
                        kind: weapon.kind,
                        roll,
                    },
                });
            } else {
                let damage = Damage {
                    source: Some(source),
                    target,
                    amount,
                    kind: weapon.kind,
                    cause: DamageCause::Attack { roll, rank },
                    action: Some(action),
                    depth: 0,
                    hit: None,
                };
                // A slow weapon's hit lands later, as far from its target as it shot.
                let aimed = index.get(target).and_then(|target| aims.get(target).ok());
                let delay = aimed.and_then(|(pos, _, aimed_body)| {
                    let at = Body::center(aimed_body, pos.get(), &map);
                    let distance = (at - Body::center(body, from.get(), &map)).length();
                    weapon.delay(distance, clock.rate().hz().get())
                });
                match delay {
                    Some(delay) => held
                        .as_mut()
                        .expect("a server's match, which strikes, holds its held hits")
                        .hold(now.after(delay), damage),
                    None => queue.push_damage(damage, now),
                }
            }
            let mut held = resources.of_mut(owner);
            let payer = Payer::of(pools, held.as_deref_mut());
            payer.pay(&values.cost, resource_cost);
        }
    }

    /// The weapon of the attack `progress` has under way, of a unit with `slots`.
    fn wielded<'a>(
        book: &'a ActionBook,
        weapons: &'a ByAction<Weapon>,
        slots: &ActionSlots,
        progress: &ActionProgress,
    ) -> Wielded<'a> {
        let underway = progress.in_progress().expect("an attack is under way");
        let slot = slots.slot(underway.slot()).expect("an attack's slot");
        let id = slot.action.expect("an attack's slot holds its weapon");
        let action = book.get(id).expect("a slot's action is in the book");
        let rank = slot.rank.expect("an attack's weapon is learned");
        Wielded {
            slot: underway.slot(),
            action: id,
            rank,
            weapon: weapons.get(id).expect("an attack's action is a weapon"),
            values: *action.values(rank),
            resource_cost: action.resource_cost(rank),
            projectile: action.delivery.map(|delivery| delivery.unit_type),
        }
    }
}
