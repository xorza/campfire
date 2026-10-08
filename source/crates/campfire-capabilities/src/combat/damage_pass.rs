use bevy_ecs::entity::Entity;
use bevy_ecs::system::Local;
use bevy_ecs::world::{Mut, World};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_sim::{EntityIndex, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_target::ActionTarget;
use crate::actions::effect_lists::EffectLists;
use crate::actions::lists_of::ListsOf;
use crate::combat::assist_window::AssistWindow;
use crate::combat::combat_bindings::CombatBindings;
use crate::combat::combat_event::{CombatEvent, CombatEvents};
use crate::combat::damage::{Damage, DamageWeigher};
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::heal::{Heal, HealCause, HealWeigher};
use crate::combat::pass_queue::{PassEntry, PassQueue};
use crate::combat::recent_attack::RecentAttack;
use crate::combat::recent_attackers::RecentAttackers;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::hook::Hook;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::life_pool::LifePool;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_id::StatId;
use crate::stats::unit_stats::UnitStats;
use crate::units::block::Block;
use crate::units::tag_book::TagBook;
use crate::units::unit_tags::UnitTags;
use crate::units::view::View;

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

/// The match's scripts the pass runs, each when the match has it, out of the world while the
/// pass runs.
#[derive(Debug)]
struct PassScripts {
    weigher: Option<DamageWeigher>,
    healer: Option<HealWeigher>,
    events: Option<CombatEvents>,
    ctx: Option<Ctx>,
}

/// What the pass runs in: the world alone, for a match with none of its scripts, or a script
/// batch.
#[derive(Debug)]
enum PassRun<'a, 'w> {
    Plain(&'a mut World),
    Scripted(&'a mut ScriptBatch<'w>),
}

impl<'w> PassRun<'_, 'w> {
    fn world(&mut self) -> &mut World {
        match self {
            PassRun::Plain(world) => world,
            PassRun::Scripted(batch) => batch.world(),
        }
    }

    fn batch(&mut self) -> Option<&mut ScriptBatch<'w>> {
        match self {
            PassRun::Plain(_) => None,
            PassRun::Scripted(batch) => Some(batch),
        }
    }
}

impl DamagePass {
    /// Applies the tick's damage and heals in the queue's order, with the units as the pass began:
    /// each damage through the mode's `calc_damage` when it has one, then an attack's weapon's
    /// `on_hit` list, then its combat events, whose damage joins the end of the queue, and whose
    /// heals, as leech's, are dealt next; each heal through the mode's `calc_heal` when it has one.
    /// Damage to a unit at zero life, or to an invulnerable one, does nothing, and so does a heal
    /// of a unit at zero life.
    pub(crate) fn run(world: &mut World, mut assisters: Local<'_, Vec<StableId>>) {
        let now = world.resource::<SimTick>().start();
        world.resource_mut::<Deaths>().clear(now);
        world.resource_mut::<PassQueue>().begin();
        let scripts = PassScripts {
            weigher: world.remove_non_send::<DamageWeigher>(),
            healer: world.remove_non_send::<HealWeigher>(),
            events: world.remove_non_send::<CombatEvents>(),
            ctx: world.get_non_send::<Ctx>().cloned(),
        };
        let scripted =
            scripts.weigher.is_some() || scripts.healer.is_some() || scripts.events.is_some();
        if !scripted {
            DamagePass::drain(&mut PassRun::Plain(world), &scripts, now, &mut assisters);
        } else if !world.resource::<PassQueue>().is_empty() {
            let view = world.non_send::<View>().clone();
            ScriptBatch::run(world, &view, |batch| {
                let run = &mut PassRun::Scripted(batch);
                DamagePass::drain(run, &scripts, now, &mut assisters);
            });
        }
        let PassScripts {
            weigher,
            healer,
            events,
            ctx: _,
        } = scripts;
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

    /// Deals the queue's entries in order, with the pass's `scripts` when `run` runs them.
    fn drain(
        run: &mut PassRun<'_, '_>,
        scripts: &PassScripts,
        now: Tick,
        assisters: &mut Vec<StableId>,
    ) {
        while let Some(entry) = run.world().resource_mut::<PassQueue>().next() {
            let damage = match entry {
                PassEntry::Damage(damage) => damage,
                PassEntry::Heal(heal) => {
                    let Some(entity) = DamagePass::living(run.world(), heal.target) else {
                        continue;
                    };
                    let amount = match (&scripts.healer, run.batch()) {
                        (Some(healer), Some(batch)) => {
                            healer.call(batch, heal).unwrap_or_else(|error| {
                                batch.record(Some(heal.target), Hook::CalcHeal, error);
                                heal.amount
                            })
                        }
                        _ => heal.amount,
                    };
                    DamagePass::heal(run.world(), entity, amount);
                    continue;
                }
            };
            if DamagePass::damageable(run.world(), damage.target).is_none() {
                continue;
            }
            let amount = match (&scripts.weigher, run.batch()) {
                (Some(weigher), Some(batch)) => {
                    weigher.call(batch, damage).unwrap_or_else(|error| {
                        batch.record(Some(damage.target), Hook::CalcDamage, error);
                        damage.amount
                    })
                }
                _ => damage.amount,
            };
            let landed = DamagePass::deal(run.world(), damage, amount, now);
            let Some(batch) = run.batch() else {
                continue;
            };
            if landed != Landed::Nothing
                && let Some(ctx) = &scripts.ctx
            {
                DamagePass::run_weapon_list(batch, ctx, damage, now);
            }
            if let Some(events) = &scripts.events {
                let dealt = Damage {
                    amount: amount.max(Num::ZERO),
                    ..damage
                };
                DamagePass::answer(batch, events, dealt, landed, assisters);
            }
        }
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
            .filter(|&entity| !UnitTags::properties_of(world.get(entity)).blocks(Block::Damage))
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
        batch.hook_call(ctx, now, start, Hook::OnHit, Some(source), |batch| {
            let target = ActionTarget::Unit(damage.target);
            let lists = ListsOf::Action(weapon);
            let frame = &mut ctx.frame();
            EffectLists::queue(batch.world(), lists, Hook::OnHit, frame, ctx.view(), target)
        });
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
            events.call(batch, CombatEvent::AttackHit(damage));
        }
        events.call(batch, CombatEvent::DamageTaken(damage));
        let Landed::Killed { death } = landed else {
            return;
        };
        let kill = batch.world().resource::<Deaths>().get(death);
        let killer = kill.killer;
        assisters.clear();
        assisters.extend_from_slice(kill.assisters);
        let (victim, depth) = (damage.target, damage.depth);
        if let Some(killer) = killer {
            events.call(
                batch,
                CombatEvent::Kill {
                    killer,
                    victim,
                    depth,
                },
            );
        }
        for unit in killer.into_iter().chain(assisters.iter().copied()) {
            events.call(
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
        let clocks = world.get::<ModifierClocks>(entity);
        if clocks.is_some_and(ModifierClocks::shielded) {
            let takes_effect = TagBook::effective(world, entity);
            let book = world.resource::<ModifierBook>().clone();
            if let Some(mut carried) = CarriedMut::of(world, entity) {
                left = carried.absorb(left, |id| takes_effect(book.tags(id)));
            }
        }
        let LifePool(life) = *world.resource::<LifePool>();
        let living = |world: &World| {
            let pools = world.get::<Pools>(entity);
            pools.expect("a unit that takes damage").above_zero(life)
        };
        // A write marks the pools changed, so damage that takes nothing writes nothing.
        let taken = if left > Num::ZERO && living(world) {
            let mut pools = world
                .get_mut::<Pools>(entity)
                .expect("a unit that takes damage");
            pools.take(life, left)
        } else {
            Num::ZERO
        };
        let killed = !living(world);
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

    /// Heals the life pool of `entity`, a living unit, by `amount` times one plus its
    /// `heal_scale` stat.
    fn heal(world: &mut World, entity: Entity, amount: Num) {
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
    pub(super) fn restore(world: &mut World, unit: StableId, pool: PoolId, amount: Num) {
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

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::StableId;

    use crate::combat::damage_pass::DamagePass;

    impl DamagePass {
        /// Heals `unit` at once, as the pass heals it, when it exists and its life is above zero.
        pub(crate) fn heal_unit(world: &mut World, unit: StableId, amount: Num) {
            if let Some(entity) = DamagePass::living(world, unit) {
                DamagePass::heal(world, entity, amount);
            }
        }
    }
}
