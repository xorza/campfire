use std::fmt;

use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, SystemState};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionBook, ActionProgress, ActionTarget, EffectListBook, ListCall, ListHook, ListsOf,
    ProgressDues,
};
use campfire_common::Tick;
use campfire_math::{Num, Rounding};
use campfire_script::{
    ActionCast, CallStart, Ctx, Hook, HookDecl, ScriptBatch, ScriptRole, ScriptsWorld,
};
use campfire_sim::{EntityIndex, SimTick, SimWorld, StableId};
use campfire_stats::{
    CarriedMut, ModifierBook, ModifierClocks, ModifierEvents, ModifierHooks, StatsWorld, UnitStats,
};
use campfire_units::{DamageHit, DamageHits, Deaths, Fallen, Life, StatId, TagEffect};
use campfire_values::{Block, NameId, UnitTags};

use crate::combat_book::CombatBook;
use crate::combat_event::CombatEvent;
use crate::damage::{Damage, WeaponHit};
use crate::damage_handle::DamageHandle;
use crate::heal::{Heal, HealCause};
use crate::heal_handle::HealHandle;
use crate::next_damage::NextDamage;
use crate::pass_queue::{PassEntry, PassQueue};
use crate::recent_attack::RecentAttack;
use crate::recent_attackers::RecentAttackers;
use crate::type_armor::TypeArmor;
use crate::weighers::{Weigh, Weighers};

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

/// What the pass keeps between its runs: the query that records a damage's source among its
/// target's attackers, made as the first record needs it, and the assisters of a death, then of
/// the kill whose events run.
#[derive(Default)]
pub(crate) struct DamageScratch {
    records: Option<SystemState<AttackerRecords<'static, 'static>>>,
    assisters: Vec<StableId>,
}

/// What a record of an attacker reads and writes: the stable ids, and each unit's attackers.
type AttackerRecords<'w, 's> = (
    Res<'w, EntityIndex>,
    Query<'w, 's, &'static mut RecentAttackers>,
);

impl fmt::Debug for DamageScratch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DamageScratch")
            .field("assisters", &self.assisters)
            .finish_non_exhaustive()
    }
}

/// `calc_damage(ctx, d)`: the mode's pure hook that gives a damage's amount.
#[derive(Debug)]
pub struct CalcDamage;

/// `calc_heal(ctx, h)`: the mode's pure hook that gives a heal's amount before the heal scale.
#[derive(Debug)]
pub struct CalcHeal;

impl HookDecl for CalcDamage {
    const NAME: &'static str = "calc_damage";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "d"];
    type Args = (Ctx, DamageHandle);
}

impl HookDecl for CalcHeal {
    const NAME: &'static str = "calc_heal";
    const ROLE: ScriptRole = ScriptRole::Mode;
    const PARAMS: &'static [&'static str] = &["ctx", "h"];
    type Args = (Ctx, HealHandle);
}

impl DamagePass {
    /// Applies the tick's damage and heals in the queue's order, with the units as the pass began:
    /// each damage through the mode's `calc_damage` when it has one, then an attack's weapon's
    /// `on_hit` list, then its combat events, whose damage joins the end of the queue, and whose
    /// heals, as leech's, are dealt next; each heal through the mode's `calc_heal` when it has one.
    /// Damage to a unit that does not live, or to an invulnerable one, does nothing, and so does a
    /// heal of a unit that does not live.
    pub(crate) fn run(world: &mut World, mut scratch: Local<'_, DamageScratch>) {
        let now = world.resource::<SimTick>().start();
        world.resource_mut::<Deaths>().clear(now);
        world.resource_mut::<DamageHits>().clear();
        // Only a tick that carries entries writes the next tick's damage, which is state.
        if world.resource::<NextDamage>().is_empty() {
            world.resource_mut::<PassQueue>().begin([]);
        } else {
            world.resource_scope(|world, mut carried: Mut<'_, NextDamage>| {
                world.resource_mut::<PassQueue>().begin(carried.drain());
            });
        }
        let hooks = ModifierEvents::of(world);
        let weigh = world.resource::<Weighers>().get();
        if weigh.is_none() && hooks.is_none() {
            Self::drain(
                &mut PassRun::Plain(world),
                hooks.as_ref(),
                weigh,
                &mut scratch,
            );
        } else if !world.resource::<PassQueue>().is_empty() {
            world.script_batch(|batch| {
                let run = &mut PassRun::Scripted(batch);
                Self::drain(run, hooks.as_ref(), weigh, &mut scratch);
            });
        }
        world.resource_mut::<PassQueue>().end(now);
    }

    /// Queues `entry` for the tick's pass, or, once that ran, as a mode's hook deals one after
    /// it, for the next tick's, which deals it first.
    pub fn queue(world: &mut World, entry: PassEntry) {
        let now = world.resource::<SimTick>().start();
        let mut queue = world.resource_mut::<PassQueue>();
        if !queue.open_at(now) {
            world.resource_mut::<NextDamage>().push(entry);
            return;
        }
        match entry {
            PassEntry::Damage(damage) => queue.push_damage(damage, now),
            PassEntry::Heal(heal) => queue.push_heal(heal, now),
        }
    }

    /// Deals the queue's entries in order, with the pass's `hooks` and the mode's `weigh` when
    /// `run` runs them, and its buffers, `scratch`.
    fn drain(
        run: &mut PassRun<'_, '_>,
        hooks: Option<&ModifierHooks>,
        weigh: Option<Weigh>,
        scratch: &mut DamageScratch,
    ) {
        let now = run.world().resource::<SimTick>().start();
        while let Some(entry) = run.world().resource_mut::<PassQueue>().next() {
            let damage = match entry {
                PassEntry::Damage(damage) => damage,
                PassEntry::Heal(heal) => {
                    let Some(entity) = Self::living(run.world(), heal.target) else {
                        continue;
                    };
                    let amount = match (weigh, run.batch()) {
                        (Some(weigh), Some(batch)) => match (weigh.heal)(batch, heal) {
                            Some(Ok(amount)) => amount,
                            Some(Err(error)) => {
                                batch.record(Some(heal.target), Hook::of::<CalcHeal>(), &error);
                                heal.amount
                            }
                            None => heal.amount,
                        },
                        _ => heal.amount,
                    };
                    Self::heal(run.world(), entity, amount);
                    continue;
                }
            };
            let Some(target) = Self::damageable(run.world(), damage.target) else {
                continue;
            };
            let damage = Damage {
                amount: Self::armored(run.world(), target, damage),
                ..damage
            };
            let amount = match (weigh, run.batch()) {
                (Some(weigh), Some(batch)) => match (weigh.damage)(batch, damage) {
                    Some(Ok(amount)) => amount,
                    Some(Err(error)) => {
                        batch.record(Some(damage.target), Hook::of::<CalcDamage>(), &error);
                        damage.amount
                    }
                    None => damage.amount,
                },
                _ => damage.amount,
            };
            let weighed = Damage { amount, ..damage };
            let landed = Self::deal(run.world(), weighed, scratch);
            let Some(batch) = run.batch() else {
                continue;
            };
            if landed != Landed::Nothing {
                let ctx = batch.ctx().clone();
                Self::run_weapon_list(batch, &ctx, damage, now);
            }
            if let Some(events) = hooks {
                let dealt = Damage {
                    amount: amount.max(Num::ZERO),
                    ..damage
                };
                Self::answer(batch, events, dealt, landed, &mut scratch.assisters);
            }
        }
    }

    /// The entity of `id`, when it exists, the sim simulates it, it has a life and it lives, as
    /// `StatsWorld::lives` says: one that damage, heals and restores reach, and write. A
    /// predicting client writes none of a unit it holds as the server sent it, which
    /// `Unpredicted` hides.
    pub(super) fn living(world: &World, id: StableId) -> Option<Entity> {
        let entity = world.queried_entity_of(id)?;
        (world.get::<Life>(entity).is_some() && world.lives(entity)).then_some(entity)
    }

    /// What `damage` deals to `target` through the armor its type gives its tags, before the mode
    /// weighs it: the whole for none.
    fn armored(world: &World, target: Entity, damage: Damage) -> Num {
        TypeArmor::of_unit(world, target).map_or(damage.amount, |armor| {
            armor.take(damage.kind, damage.amount)
        })
    }

    /// The entity of `id`, when it is living and its tags let damage reach it.
    fn damageable(world: &World, id: StableId) -> Option<Entity> {
        Self::living(world, id).filter(|&entity| {
            let tags = world.get::<UnitTags>(entity);
            !tags.is_some_and(|tags| tags.properties.blocks(Block::Damage))
        })
    }

    /// Runs the `on_hit` list of the weapon of `damage`, an attack's that reached its target, in
    /// one call: to the target, from the attacker, at the weapon's rank, with the attack's hit, one
    /// link down the chain of combat events, as a hook its events cause runs; its effects apply as
    /// a hook's do. A call at `ScriptLimits::CHAIN_DEPTH`, or one that fails, applies nothing and
    /// is recorded. Any other damage has no list.
    fn run_weapon_list(batch: &mut ScriptBatch<'_>, ctx: &Ctx, damage: Damage, now: Tick) {
        let (Some(WeaponHit { rank, allegiance }), Some(weapon)) =
            (damage.cause.weapon_hit(), damage.action)
        else {
            return;
        };
        let world = batch.world();
        if world
            .resource::<EffectListBook>()
            .of(ListsOf::Action(weapon, ListHook::OnHit))
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
        let cast = ActionCast {
            action: weapon,
            rank,
            caster: source,
            allegiance,
            package,
        };
        let start = CallStart {
            depth: damage.depth.saturating_add(1),
            hit: damage.hit,
            ..CallStart::cast(cast, now)
        };
        batch.answer(start, ListHook::OnHit.hook(), Some(source), |batch| {
            let call = ListCall {
                of: ListsOf::Action(weapon, ListHook::OnHit),
                reached: ActionTarget::Unit(damage.target),
            };
            EffectListBook::queue(batch.world(), call, ctx)
        });
    }

    /// Runs the events of `damage`, which `landed`, its amount after `calc_damage`: an attack's
    /// hit, the damage taken, then a kill's: the killer's, then the takedowns of the killer and
    /// each assister, by stable id, whom `assisters` holds while their hooks run.
    fn answer(
        batch: &mut ScriptBatch<'_>,
        events: &ModifierHooks,
        damage: Damage,
        landed: Landed,
        assisters: &mut Vec<StableId>,
    ) {
        if landed == Landed::Nothing {
            return;
        }
        if damage.cause.is_attack() {
            CombatEvent::AttackHit(damage).reach(events, batch);
        }
        CombatEvent::DamageTaken(damage).reach(events, batch);
        let Landed::Killed { death } = landed else {
            return;
        };
        let kill = batch.world().resource::<Deaths>().get(death);
        let killer = kill.killer;
        assisters.clear();
        assisters.extend_from_slice(kill.assisters);
        let (victim, depth) = (damage.target, damage.depth);
        if let Some(killer) = killer {
            CombatEvent::Kill {
                killer,
                victim,
                depth,
            }
            .reach(events, batch);
        }
        for unit in killer.into_iter().chain(assisters.iter().copied()) {
            CombatEvent::Takedown {
                unit,
                victim,
                depth,
            }
            .reach(events, batch);
        }
    }

    /// Deals `damage`, its amount weighed, in this tick, a negative amount as 0: more than 0 cuts a
    /// channel that damage interrupts, then shields absorb it, and the life pool takes the rest.
    /// The source is recorded as the target's attacker; when the damage took the target to zero,
    /// the source is its killer if it still exists, and the others that damaged it within the
    /// assist window assisted. A living source heals by its `leech` stat, `attack` for an attack's
    /// damage and `other` for the rest, times the life taken. Each write follows the reads it
    /// needs, which go to `scratch`.
    fn deal(world: &mut World, damage: Damage, scratch: &mut DamageScratch) -> Landed {
        let Some(entity) = Self::damageable(world, damage.target) else {
            return Landed::Nothing;
        };
        let now = world.resource::<SimTick>().start();
        let index = world.resource::<EntityIndex>();
        let source = damage.source.filter(|&source| index.get(source).is_some());
        let mut left = damage.amount.max(Num::ZERO);
        let progress = world.get::<ActionProgress>(entity);
        let book = world.resource::<ActionBook>();
        if left > Num::ZERO && progress.is_some_and(|progress| progress.damage_cuts(book)) {
            ProgressDues::change(world, damage.target, entity, |progress, dues| {
                progress.cut_channel(dues);
            })
            .expect("a unit whose channel runs");
        }
        let clocks = world.get::<ModifierClocks>(entity);
        if clocks.is_some_and(ModifierClocks::is_shielded) {
            let takes_effect = TagEffect::of(world, entity);
            let absorbed = world.resource_scope(|world, book: Mut<'_, ModifierBook>| {
                CarriedMut::with(world, entity, |carried| {
                    carried.absorb(left, |id| takes_effect.takes(book.tags(id)))
                })
            });
            left = absorbed.unwrap_or(left);
        }
        let living = |world: &World| {
            let life = world.get::<Life>(entity);
            !life.expect("a unit that takes damage").is_empty()
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
            let target = damage.target;
            world
                .resource_mut::<DamageHits>()
                .push(DamageHit { target, source });
            let records = scratch
                .records
                .get_or_insert_with(|| SystemState::new(world));
            let (index, mut attackers) = records
                .get_mut(world)
                .expect("the records of attackers are always valid");
            if let Ok(mut attackers) = attackers.get_mut(entity) {
                attackers.record(source, now, &index);
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
            let fallen = Fallen::of(world.entity(entity));
            let mut deaths = world.resource_mut::<Deaths>();
            deaths.push(fallen, source, scratch.assisters.iter().copied())
        });
        if let Some((id, entity)) =
            source.and_then(|id| Self::living(world, id).map(|entity| (id, entity)))
        {
            let bindings = *world.resource::<CombatBook>();
            let stat = if damage.cause.is_attack() {
                bindings.leech_attack
            } else {
                bindings.leech_other
            };
            let amount =
                taken.saturating_mul(Self::stat(world, entity, stat), Rounding::NearestEven);
            if amount > Num::ZERO {
                let heal = Heal {
                    source: Some(id),
                    target: id,
                    amount,
                    cause: HealCause::Leech,
                    action: damage.action,
                    depth: damage.depth,
                };
                world.resource_mut::<PassQueue>().push_heal(heal, now);
            }
        }
        death.map_or(Landed::Taken, |death| Landed::Killed { death })
    }

    /// Heals the life pool of `entity`, a living unit, by `amount` times one plus its
    /// `heal_scale` stat.
    pub(super) fn heal(world: &mut World, entity: Entity, amount: Num) {
        let bindings = *world.resource::<CombatBook>();
        let scale = Self::stat(world, entity, bindings.heal_scale);
        let received = Num::ONE.checked_add(scale).unwrap_or(Num::MAX);
        let amount = amount.saturating_mul(received, Rounding::NearestEven);
        if amount > Num::ZERO {
            let mut life = world.get_mut::<Life>(entity).expect("a living unit");
            life.add(amount);
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
