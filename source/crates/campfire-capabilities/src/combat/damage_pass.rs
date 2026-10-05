use bevy_ecs::entity::Entity;
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{EntityIndex, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::effect_lists::{EffectLists, ListsOf};
use crate::combat::assist_window::AssistWindow;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::combat_effect::CombatEffect;
use crate::combat::combat_event::CombatEvent;
use crate::combat::combat_events::CombatEvents;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_weigher::DamageWeigher;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::heal::{Heal, HealCause};
use crate::combat::heal_weigher::HealWeigher;
use crate::combat::pass_queue::{PassEntry, PassQueue};
use crate::combat::recent_attack::RecentAttack;
use crate::combat::recent_attackers::RecentAttackers;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;

use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::life_pool::LifePool;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_id::StatId;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::script_view::View;
use crate::units::tag_book::TagBook;
use crate::units::unit_tags::UnitTags;

/// The tick's damage pass: the queue of damage and heals, dealt in order, with the combat events
/// they cause.
#[derive(Debug)]
pub(crate) struct DamagePass;

/// What a damage of the pass did: nothing, as to a unit at zero life or an invulnerable one;
/// damage; or a kill, which the tick's deaths record at `death`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landed {
    Nothing,
    Taken,
    Killed { death: usize },
}

impl DamagePass {
    /// Applies the tick's damage and heals in the queue's order, with the units as the pass began:
    /// each damage through the mode's `calc_damage` when it has one, then an attack's weapon's
    /// `on_hit` list, then its combat events, whose damage joins the end of the queue, and whose
    /// heals, as leech's, are dealt next; each heal through the mode's `calc_heal` when it has
    /// one.
    /// Damage to a unit at zero life, or to an invulnerable one, does nothing, and so does a heal of a
    /// unit at zero life.
    pub(crate) fn run(world: &mut World, mut assisters: Local<'_, Vec<StableId>>) {
        let now = world.resource::<SimTick>().start();
        world.resource_mut::<Deaths>().clear(now);
        world.resource_mut::<PassQueue>().begin();
        let weigher = world.remove_non_send::<DamageWeigher>();
        let healer = world.remove_non_send::<HealWeigher>();
        let events = world.remove_non_send::<CombatEvents>();
        let ctx = world.get_non_send::<Ctx>().cloned();
        if weigher.is_none() && healer.is_none() && events.is_none() {
            while let Some(entry) = world.resource_mut::<PassQueue>().next() {
                match entry {
                    PassEntry::Damage(damage) => {
                        DamagePass::deal(world, damage, damage.amount, now);
                    }
                    PassEntry::Heal(heal) => DamagePass::heal(world, heal.target, heal.amount),
                }
            }
        } else if !world.resource::<PassQueue>().is_empty() {
            let view = world.non_send::<View>().clone();
            ScriptBatch::run(world, &view, |batch| {
                while let Some(entry) = batch.world().resource_mut::<PassQueue>().next() {
                    let damage = match entry {
                        PassEntry::Damage(damage) => damage,
                        PassEntry::Heal(heal) => {
                            if DamagePass::living(batch.world(), heal.target).is_none() {
                                continue;
                            }
                            let amount = healer.as_ref().map_or(heal.amount, |healer| {
                                healer.weigh(batch, heal).unwrap_or_else(|error| {
                                    batch.record(Some(heal.target), Hook::CalcHeal, error);
                                    heal.amount
                                })
                            });
                            DamagePass::heal(batch.world(), heal.target, amount);
                            continue;
                        }
                    };
                    if DamagePass::damageable(batch.world(), damage.target).is_none() {
                        continue;
                    }
                    let amount = weigher.as_ref().map_or(damage.amount, |weigher| {
                        weigher.weigh(batch, damage).unwrap_or_else(|error| {
                            batch.record(Some(damage.target), Hook::CalcDamage, error);
                            damage.amount
                        })
                    });
                    let landed = DamagePass::deal(batch.world(), damage, amount, now);
                    if landed != Landed::Nothing
                        && let Some(ctx) = &ctx
                    {
                        DamagePass::run_weapon_list(batch, ctx, damage, now);
                    }
                    if let Some(events) = &events {
                        let dealt = Damage {
                            amount: amount.max(Num::ZERO),
                            ..damage
                        };
                        DamagePass::answer(batch, events, dealt, landed, &mut assisters);
                    }
                }
            });
        }
        if let Some(weigher) = weigher {
            world.insert_non_send(weigher);
        }
        if let Some(healer) = healer {
            world.insert_non_send(healer);
        }
        if let Some(events) = events {
            world.insert_non_send(events);
        }
        world.resource_mut::<PassQueue>().end();
    }

    /// The entity of `unit`, when it exists and its life pool is above zero: one that damage,
    /// heals and restores reach.
    fn living(world: &World, unit: StableId) -> Option<Entity> {
        let entity = world.resource::<EntityIndex>().get(unit)?;
        let LifePool(life) = *world.resource::<LifePool>();
        world
            .get::<Pools>(entity)?
            .above_zero(life)
            .then_some(entity)
    }

    /// The entity of `unit`, when it is living and its tags let damage reach it.
    fn damageable(world: &World, unit: StableId) -> Option<Entity> {
        DamagePass::living(world, unit)
            .filter(|&entity| !UnitTags::effects_of(world.get(entity)).blocks(Block::Damage))
    }

    /// Runs the `on_hit` list of the weapon of `damage`, an attack's that reached its target, in
    /// one call: to the target, from the attacker, at the weapon's rank, with the attack's hit, one
    /// link down the chain of combat events, as a hook its events cause runs; its effects apply as
    /// a hook's do. A call at `ScriptLimits::CHAIN_DEPTH`, or one that fails, applies nothing and
    /// is recorded. Any other damage has no list.
    fn run_weapon_list(batch: &mut ScriptBatch<'_>, ctx: &Ctx, damage: Damage, now: Tick) {
        let (Some(rank), Some(weapon)) = (damage.cause.weapon_rank(), damage.ability) else {
            return;
        };
        let world = batch.world();
        if world
            .resource::<EffectLists>()
            .of(ListsOf::Action(weapon), Hook::OnHit)
            .is_empty()
        {
            return;
        }
        let source = damage
            .source
            .expect("an attack of a weapon names its attacker");
        let package = world
            .resource::<ActionBook>()
            .get(weapon)
            .expect("a weapon is in the book")
            .package;
        let start = CallStart {
            depth: damage.depth.saturating_add(1),
            hit: damage.hit,
            ..CallStart::cast(weapon, rank, source, package)
        };
        let queued = {
            let mut frame = ctx.frame();
            frame.begin(world, start).and_then(|()| {
                let target = ActionTarget::Unit(damage.target);
                let lists = ListsOf::Action(weapon);
                EffectLists::queue(world, lists, Hook::OnHit, &mut frame, ctx.view(), target)
            })
        };
        match queued {
            Ok(()) => ctx.apply(batch.world(), now),
            Err(error) => batch.record(Some(source), Hook::OnHit, error),
        }
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
        let Landed::Killed { death } = landed else {
            return;
        };
        let kill = batch.world().resource::<Deaths>().get(death);
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
    /// the life pool takes the rest. The source is recorded as the target's attacker; when the
    /// damage took the target to zero, the source is its killer if it still exists, and the others
    /// that damaged it within the assist window assisted. A living source heals by its `leech`
    /// stat, `attack` for an attack's damage and `other` for the rest, times the life taken.
    fn deal(world: &mut World, damage: Damage, amount: Num, now: Tick) -> Landed {
        let Some(entity) = DamagePass::damageable(world, damage.target) else {
            return Landed::Nothing;
        };
        let index = world.resource::<EntityIndex>();
        let source = damage.source.filter(|&source| index.get(source).is_some());
        let mut left = amount.max(Num::ZERO);
        let takes_effect = TagBook::effective(world, entity);
        let book = world.get_resource::<ModifierBook>().cloned();
        if let (Some(mut carried), Some(book)) = (CarriedMut::of(world, entity), book) {
            left = carried.absorb(left, |id| takes_effect(book.tags(id)));
        }
        let LifePool(life) = *world.resource::<LifePool>();
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
        let death = killed.then(|| {
            world.resource_scope(|world, mut deaths: Mut<'_, Deaths>| {
                let window = world.get_resource::<AssistWindow>().map(|window| window.0);
                let index = world.resource::<EntityIndex>();
                let assisted = |attack: &RecentAttack| {
                    let within = window
                        .zip(now.since(attack.tick))
                        .is_some_and(|(window, since)| since <= window);
                    let exists = index.get(attack.source).is_some();
                    Some(attack.source) != damage.source && within && exists
                };
                let assisters = world
                    .get::<RecentAttackers>(entity)
                    .into_iter()
                    .flat_map(RecentAttackers::iter)
                    .filter(assisted)
                    .map(|attack| attack.source);
                let fallen = Fallen::of(damage.target, world.get(entity), world.get(entity));
                deaths.push(fallen, source, assisters)
            })
        });
        if let Some((id, entity)) =
            source.and_then(|id| DamagePass::living(world, id).map(|entity| (id, entity)))
        {
            let bindings = world.resource::<CombatBindings>();
            let stat = if damage.cause.attack() {
                bindings.leech_attack
            } else {
                bindings.leech_other
            };
            let amount = scaled(taken, DamagePass::stat(world, entity, stat));
            if amount > Num::ZERO {
                world.resource_mut::<PassQueue>().push_heal(Heal {
                    source: Some(id),
                    target: id,
                    amount,
                    cause: HealCause::Leech,
                    ability: damage.ability,
                    depth: damage.depth,
                });
            }
        }
        death.map_or(Landed::Taken, |death| Landed::Killed { death })
    }

    /// Applies `effect`, which the call in `frame` queued: from its acting unit, by its ability,
    /// at its chain depth, delivered by its hit. Damage and a heal join the pass's queue, a
    /// restore applies at once, and an extra attack queues the source's attack damage, when it
    /// still has an attack.
    pub(super) fn apply_effect(world: &mut World, effect: CombatEffect, frame: &Frame) {
        let (source, ability, depth) = (frame.acting(), frame.action(), frame.depth());
        let damage = |target, amount, kind, cause| Damage {
            source,
            target,
            amount,
            kind,
            cause,
            ability,
            depth,
            hit: frame.hit(),
        };
        match effect {
            CombatEffect::Damage {
                target,
                amount,
                kind,
            } => {
                let damage = damage(target, amount, kind, DamageCause::Effect);
                world.resource_mut::<PassQueue>().push_damage(damage);
            }
            CombatEffect::Heal { unit, amount } => {
                world.resource_mut::<PassQueue>().push_heal(Heal {
                    source,
                    target: unit,
                    amount,
                    cause: HealCause::Effect,
                    ability,
                    depth,
                });
            }
            CombatEffect::Restore { unit, pool, amount } => {
                DamagePass::restore(world, unit, pool, amount);
            }
            CombatEffect::AttackHit { target } => {
                let index = world.resource::<EntityIndex>();
                let Some(unit) = source.and_then(|source| index.get(source)) else {
                    return;
                };
                let unit = world.entity(unit);
                let book = world.resource::<ActionBook>();
                let first = unit.get::<ActionSlots>().and_then(|slots| {
                    let slot = slots.slot(book.weapon_for(slots, None)?)?;
                    let weapon = book.get(slot.action?)?.kind.weapon()?;
                    Some((slot, weapon))
                });
                let Some((slot, weapon)) = first else {
                    return;
                };
                let stats = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
                let hit = Damage {
                    ability: slot.action,
                    ..damage(
                        target,
                        weapon.damage(stats),
                        weapon.kind,
                        DamageCause::ExtraAttack { rank: slot.rank },
                    )
                };
                world.resource_mut::<PassQueue>().push_damage(hit);
            }
        }
    }

    /// Heals `unit`'s life pool by `amount` times one plus its `heal_scale` stat, when it exists
    /// and its life is above zero.
    pub(crate) fn heal(world: &mut World, unit: StableId, amount: Num) {
        if let Some(entity) = DamagePass::living(world, unit) {
            DamagePass::heal_living(world, entity, amount);
        }
    }

    /// Heals `entity`, a living unit, as `heal` does.
    fn heal_living(world: &mut World, entity: Entity, amount: Num) {
        let bindings = *world.resource::<CombatBindings>();
        let LifePool(life) = *world.resource::<LifePool>();
        let scale = DamagePass::stat(world, entity, bindings.heal_scale);
        let received = Num::ONE.checked_add(scale).unwrap_or(Num::MAX);
        let amount = scaled(amount, received);
        if amount > Num::ZERO {
            let mut pools = world.get_mut::<Pools>(entity).expect("a living unit");
            pools.add(life, amount);
        }
    }

    /// Restores `amount` of `unit`'s `pool`, unscaled, when the unit exists, its life is above
    /// zero, and it has the pool.
    pub(crate) fn restore(world: &mut World, unit: StableId, pool: PoolId, amount: Num) {
        let Some(entity) = DamagePass::living(world, unit) else {
            return;
        };
        if amount > Num::ZERO {
            let mut pools = world.get_mut::<Pools>(entity).expect("a living unit");
            pools.add(pool, amount);
        }
    }

    /// `entity`'s value of the stat at `stat` among the stats; 0 when `[combat]` binds none, or
    /// the unit has no stats.
    fn stat(world: &World, entity: Entity, stat: Option<StatId>) -> Num {
        let values = world.get::<UnitStats>(entity);
        stat.zip(values)
            .map_or(Num::ZERO, |(at, values)| values.values()[at.index()])
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
