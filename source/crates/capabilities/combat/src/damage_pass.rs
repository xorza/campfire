use bevy_ecs::entity::Entity;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, EffectListBook, ListsOf};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::{CallStart, Ctx, Hook, ScriptBatch, ScriptsWorldCrate};
use campfire_sim::{EntityIndex, SimTick, SimWorld, StableId};
use campfire_stats::{
    CarriedMut, Life, ModifierBook, ModifierClocks, PoolId, StatId, StatsWorld, UnitPools,
    UnitStats,
};
use campfire_units::TagEffect;
use campfire_values::{Block, UnitTags};

use crate::combat_book::CombatBook;
use crate::combat_event::{CombatEvent, CombatEvents};
use crate::combat_hooks::CombatHooks;
use crate::damage::Damage;
use crate::deaths::{Deaths, Fallen};
use crate::heal::{Heal, HealCause};
use crate::pass_queue::{PassEntry, PassQueue};
use crate::recent_attack::RecentAttack;
use crate::recent_attackers::RecentAttackers;

/// The tick's damage pass: the queue of damage and heals, dealt in order, with the combat events
/// they cause.
#[derive(Debug)]
pub struct DamagePass;

/// What a damage of the pass did: nothing, as to a unit at zero life or an invulnerable one;
/// damage; or a kill, which the tick's deaths record at `death`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landed {
    Nothing,
    Taken,
    Killed { death: usize },
}

/// What the pass runs in: the world alone, for a match with none of its scripts, or a script
/// batch.
#[derive(Debug)]
enum PassRun<'a, 'w> {
    Plain(&'a mut World),
    Scripted(&'a mut ScriptBatch<'w>),
}

impl<'w> PassRun<'_, 'w> {
    const fn world(&mut self) -> &mut World {
        match self {
            PassRun::Plain(world) => world,
            PassRun::Scripted(batch) => batch.world(),
        }
    }

    const fn batch(&mut self) -> Option<&mut ScriptBatch<'w>> {
        match self {
            PassRun::Plain(_) => None,
            PassRun::Scripted(batch) => Some(batch),
        }
    }
}

/// The buffers the pass keeps between its runs: the attackers a damage's target forgets as its
/// source strikes it, and the assisters of a death, then of the kill whose events run.
#[derive(Debug, Default)]
pub(crate) struct PassScratch {
    gone: Vec<StableId>,
    assisters: Vec<StableId>,
}

impl DamagePass {
    /// Applies the tick's damage and heals in the queue's order, with the units as the pass began:
    /// each damage through the mode's `calc_damage` when it has one, then an attack's weapon's
    /// `on_hit` list, then its combat events, whose damage joins the end of the queue, and whose
    /// heals, as leech's, are dealt next; each heal through the mode's `calc_heal` when it has one.
    /// Damage to a unit at zero life, or to an invulnerable one, does nothing, and so does a heal
    /// of a unit at zero life.
    pub(crate) fn run(world: &mut World, mut scratch: Local<'_, PassScratch>) {
        let now = world.resource::<SimTick>().start();
        world.resource_mut::<Deaths>().clear(now);
        world.resource_mut::<PassQueue>().begin();
        let hooks = world.non_send::<CombatHooks>().clone();
        if !hooks.scripted() {
            DamagePass::drain(&mut PassRun::Plain(world), &hooks, &mut scratch);
        } else if !world.resource::<PassQueue>().is_empty() {
            world.script_batch(|batch| {
                let run = &mut PassRun::Scripted(batch);
                DamagePass::drain(run, &hooks, &mut scratch);
            });
        }
        world.resource_mut::<PassQueue>().end();
    }

    /// Deals the queue's entries in order, with the pass's `hooks` when `run` runs them, and its
    /// buffers, `scratch`.
    fn drain(run: &mut PassRun<'_, '_>, hooks: &CombatHooks, scratch: &mut PassScratch) {
        let now = run.world().resource::<SimTick>().start();
        while let Some(entry) = run.world().resource_mut::<PassQueue>().next() {
            let damage = match entry {
                PassEntry::Damage(damage) => damage,
                PassEntry::Heal(heal) => {
                    let Some(entity) = DamagePass::living(run.world(), heal.target) else {
                        continue;
                    };
                    let amount = match (&hooks.healer, run.batch()) {
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
            let amount = match (&hooks.weigher, run.batch()) {
                (Some(weigher), Some(batch)) => {
                    weigher.call(batch, damage).unwrap_or_else(|error| {
                        batch.record(Some(damage.target), Hook::CalcDamage, error);
                        damage.amount
                    })
                }
                _ => damage.amount,
            };
            let weighed = Damage { amount, ..damage };
            let landed = DamagePass::deal(run.world(), weighed, scratch);
            let Some(batch) = run.batch() else {
                continue;
            };
            if landed != Landed::Nothing {
                let ctx = batch.ctx().clone();
                DamagePass::run_weapon_list(batch, &ctx, damage, now);
            }
            if let Some(events) = &hooks.events {
                let dealt = Damage {
                    amount: amount.max(Num::ZERO),
                    ..damage
                };
                DamagePass::answer(batch, events, dealt, landed, &mut scratch.assisters);
            }
        }
    }

    /// The entity of `id`, when it exists, the sim simulates it, and its life pool is above
    /// zero: one that damage, heals and restores reach, and write. A predicting client writes
    /// none of a unit it holds as the server sent it, which `Unpredicted` hides.
    pub(super) fn living(world: &World, id: StableId) -> Option<Entity> {
        let entity = world.queried_entity_of(id)?;
        world.get::<Life>(entity)?.above_zero().then_some(entity)
    }

    /// The entity of `id`, when it is living and its tags let damage reach it.
    fn damageable(world: &World, id: StableId) -> Option<Entity> {
        DamagePass::living(world, id)
            .filter(|&entity| !UnitTags::properties_of(world.get(entity)).blocks(Block::Damage))
    }

    /// Runs the `on_hit` list of the weapon of `damage`, an attack's that reached its target, in
    /// one call: to the target, from the attacker, at the weapon's rank, with the attack's hit, one
    /// link down the chain of combat events, as a hook its events cause runs; its effects apply as
    /// a hook's do. A call at `ScriptLimits::CHAIN_DEPTH`, or one that fails, applies nothing and
    /// is recorded. Any other damage has no list.
    fn run_weapon_list(batch: &mut ScriptBatch<'_>, ctx: &Ctx, damage: Damage, now: Tick) {
        let (Some(rank), Some(weapon)) = (damage.cause.weapon_rank(), damage.action) else {
            return;
        };
        let world = batch.world();
        if world
            .resource::<EffectListBook>()
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
            ..CallStart::cast(weapon, rank, source, package, now)
        };
        batch.hook_call(start, Hook::OnHit, Some(source), |batch| {
            let target = ActionTarget::Unit(damage.target);
            let lists = ListsOf::Action(weapon);
            let frame = &mut ctx.frame();
            EffectListBook::queue(batch.world(), lists, Hook::OnHit, frame, ctx.view(), target)
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
        for id in killer.into_iter().chain(assisters.iter().copied()) {
            events.call(batch, CombatEvent::Takedown { id, victim, depth });
        }
    }

    /// Deals `damage`, its amount weighed, in this tick, a negative amount as 0: more than 0 cuts a
    /// channel that damage interrupts, then shields absorb it, and the life pool takes the rest.
    /// The source is recorded as the target's attacker; when the damage took the target to zero,
    /// the source is its killer if it still exists, and the others that damaged it within the
    /// assist window assisted. A living source heals by its `leech` stat, `attack` for an attack's
    /// damage and `other` for the rest, times the life taken. Each write follows the reads it
    /// needs, which go to `scratch`.
    fn deal(world: &mut World, damage: Damage, scratch: &mut PassScratch) -> Landed {
        let Some(entity) = DamagePass::damageable(world, damage.target) else {
            return Landed::Nothing;
        };
        let now = world.resource::<SimTick>().start();
        let index = world.resource::<EntityIndex>();
        let source = damage.source.filter(|&source| index.get(source).is_some());
        let mut left = damage.amount.max(Num::ZERO);
        let slots = world.get::<ActionSlots>(entity);
        if left > Num::ZERO
            && slots.is_some_and(|slots| slots.damage_cuts(world.resource::<ActionBook>()))
        {
            let slots = world.get_mut::<ActionSlots>(entity);
            slots.expect("a unit whose channel runs").cut_channel();
        }
        let clocks = world.get::<ModifierClocks>(entity);
        if clocks.is_some_and(ModifierClocks::shielded) {
            let takes_effect = TagEffect::of(world, entity);
            let book = world.resource::<ModifierBook>().clone();
            if let Some(mut carried) = CarriedMut::of(world, entity) {
                left = carried.absorb(left, |id| takes_effect.takes(book.tags(id)));
            }
        }
        let living = |world: &World| {
            let life = world.get::<Life>(entity);
            life.expect("a unit that takes damage").above_zero()
        };
        // A write marks the life changed, so damage that takes nothing writes nothing.
        let taken = if left > Num::ZERO && living(world) {
            let mut life = world
                .get_mut::<Life>(entity)
                .expect("a unit that takes damage");
            life.take(left)
        } else {
            Num::ZERO
        };
        let killed = !living(world);
        if let Some(source) = source {
            let index = world.resource::<EntityIndex>();
            if let Some(attackers) = world.get::<RecentAttackers>(entity) {
                attackers.gone(source, index, &mut scratch.gone);
                world
                    .get_mut::<RecentAttackers>(entity)
                    .expect("a unit with attackers")
                    .record(source, now, &scratch.gone);
            }
        }
        let death = killed.then(|| {
            let window = world.resource::<CombatBook>().assist_window;
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
            scratch.assisters.clear();
            scratch.assisters.extend(assisters);
            let fallen = Fallen::of(damage.target, world.get(entity), world.get(entity));
            let mut deaths = world.resource_mut::<Deaths>();
            deaths.push(fallen, source, scratch.assisters.iter().copied())
        });
        if let Some((id, entity)) =
            source.and_then(|id| DamagePass::living(world, id).map(|entity| (id, entity)))
        {
            let bindings = world.resource::<CombatBook>().bindings;
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
                    action: damage.action,
                    depth: damage.depth,
                });
            }
        }
        death.map_or(Landed::Taken, |death| Landed::Killed { death })
    }

    /// Heals the life pool of `entity`, a living unit, by `amount` times one plus its
    /// `heal_scale` stat.
    pub(super) fn heal(world: &mut World, entity: Entity, amount: Num) {
        let bindings = world.resource::<CombatBook>().bindings;
        let scale = DamagePass::stat(world, entity, bindings.heal_scale);
        let received = Num::ONE.checked_add(scale).unwrap_or(Num::MAX);
        let amount = scaled(amount, received);
        if amount > Num::ZERO {
            let mut life = world.get_mut::<Life>(entity).expect("a living unit");
            life.add(amount);
        }
    }

    /// Restores `amount` of `id`'s `pool`, unscaled, when the unit exists, its life is above
    /// zero, and it has the pool.
    pub fn restore(world: &mut World, id: StableId, pool: PoolId, amount: Num) {
        let Some(entity) = DamagePass::living(world, id) else {
            return;
        };
        if amount > Num::ZERO {
            let life = world.life_pool();
            let mut id = world.entity_mut(entity);
            let pools = id.get_components_mut::<UnitPools>();
            pools.expect("a unit's pools").of(life).add(pool, amount);
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
