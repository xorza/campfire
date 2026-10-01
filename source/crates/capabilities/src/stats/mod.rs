//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::{Added, Changed, Or, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, NonSend, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_math::Num;
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, Ticks};

use crate::abilities::ability_book::AbilityId;
use crate::abilities::resource_pool::ResourcePool;
use crate::combat::CombatSet;
use crate::combat::attack_stats::AttackStats;
use crate::combat::combat_events::CombatEvents;
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::navigation::move_step::MoveStep;
use crate::scripts::ctx::Ctx;
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::stats::level::Level;
use crate::stats::modifier_book::{Applier, ModifierBook, ModifierId};
use crate::stats::modifier_data::ModifierData;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifier_hooks::ModifierHooks;
use crate::stats::modifiers::Modifiers;
use crate::stats::stat::EngineStat;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::script_view::{RowFill, View};
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::scalar::Scalar;

pub(crate) mod level;
pub(crate) mod modifier_book;
pub(crate) mod modifier_data;
pub(crate) mod modifier_effect;
pub(crate) mod modifier_handle;
pub(crate) mod modifier_hooks;
pub(crate) mod modifiers;
pub(crate) mod stat;
pub(crate) mod stat_book;
pub(crate) mod stat_rule;
pub(crate) mod stats_api;
pub(crate) mod stats_data;
pub(crate) mod unit_state;
pub(crate) mod unit_stats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

/// The systems of `stats`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StatsSet {
    /// In `SimSet::Inputs`: the modifiers and stacks that hold no longer end.
    Expire,
}

impl Stats {
    /// Adds stats to a match: before the first stage and after each, every unit whose level
    /// changed, or that is new, has its stats derived again, and the components that hold their
    /// effect follow them; as each tick starts, the living units' pools regenerate. With no stat
    /// book, as before a mode loads one or on a client, which loads none, nothing changes.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(ModifierBook::default());
        if let Some(ctx) = world.get_non_send::<Ctx>().cloned() {
            let hooks = ModifierHooks::new(ctx);
            world.insert_non_send(CombatEvents::new(move |batch, event| {
                hooks.hear(batch, event);
            }));
        }
        registry.register_component::<Level>();
        registry.register_component::<Modifiers>();
        schedule.add_systems((
            (expire_modifiers.in_set(StatsSet::Expire), regenerate).in_set(SimSet::Inputs),
            (clear_dead_modifiers, apply_auras)
                .chain()
                .in_set(SimSet::Resolve)
                .after(CombatSet::Die),
            refresh_stats.before(SimSet::Inputs),
        ));
        for pair in SimSet::ALL.windows(2) {
            schedule.add_systems(refresh_stats.after(pair[0]).before(pair[1]));
        }
        schedule.add_systems(refresh_stats.after(SimSet::Vision));
    }

    /// Gives the match the mode's stat book.
    pub(crate) fn load(world: &mut World, book: StatBook) {
        world.insert_resource(book);
    }

    /// Applies `effect`, which a call by `applier` queued. An added modifier's numbers resolve
    /// now, an ability's params read through `ability_param`; nothing is added to a dead or gone
    /// unit, one that carries no modifiers, or when a number does not resolve.
    pub(crate) fn apply_effect(
        world: &mut World,
        effect: ModifierEffect,
        applier: Applier,
        ability_param: impl Fn(&str) -> Option<Scalar>,
    ) {
        match effect {
            ModifierEffect::Add {
                target,
                id,
                duration,
            } => Stats::add_modifier(world, target, id, applier, duration, ability_param),
            ModifierEffect::Remove {
                carrier,
                id,
                source,
            } => Stats::remove_modifier(world, carrier, id, source),
        }
    }

    fn add_modifier(
        world: &mut World,
        target: StableId,
        id: ModifierId,
        applier: Applier,
        duration: Option<Ticks>,
        ability_param: impl Fn(&str) -> Option<Scalar>,
    ) {
        let Some(entity) = world.resource::<EntityIndex>().get(target) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let now = world.resource::<SimTick>().start();
        let (Some(book), Some(stats)) = (
            world.get_resource::<ModifierBook>(),
            world.get_resource::<StatBook>(),
        ) else {
            return;
        };
        let Some(application) = book.application(id, applier, duration, now, stats, ability_param)
        else {
            return;
        };
        if let Some(mut modifiers) = world.get_mut::<Modifiers>(entity) {
            modifiers.apply(application);
        }
    }

    fn remove_modifier(
        world: &mut World,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) {
        let entity = world.resource::<EntityIndex>().get(carrier);
        if let Some(mut modifiers) = entity.and_then(|entity| world.get_mut::<Modifiers>(entity))
            && modifiers.bypass_change_detection().remove(id, source)
        {
            modifiers.set_changed();
        }
    }

    /// Writes what a call wrote to `handle`, its stacks and state, to its instance, if it still
    /// holds.
    pub(crate) fn write_handle(world: &mut World, handle: &ModifierHandle) {
        let data = handle.data();
        if !data.written {
            return;
        }
        let now = world.resource::<SimTick>().start();
        let entity = world.resource::<EntityIndex>().get(data.carrier);
        let Some(mut modifiers) = entity.and_then(|entity| world.get_mut::<Modifiers>(entity))
        else {
            return;
        };
        if let Some(instance) = modifiers.get_mut(data.id, data.source) {
            instance.set_stacks(data.stacks, now);
            instance.state.clone_from(&data.state);
        }
    }

    /// Loads `data` as the modifier `name` of `package`: 0 the mode, then each package it
    /// depends on, in its manifest's order, with its compiled `script` exactly when its data
    /// names one. Modifiers load by package, then name.
    pub fn load_modifier(
        world: &mut World,
        package: u16,
        name: &str,
        data: &ModifierData,
        script: Option<ScriptId>,
    ) {
        let hooks = script.map_or_else(HookSet::default, |script| {
            let host = world.non_send::<ScriptHost>();
            let defines = |&hook: &Hook| host.defines(script, hook.name(), hook.params());
            HookSet::of(MODIFIER_HOOKS.into_iter().filter(defines))
        });
        let id = world
            .resource_mut::<ModifierBook>()
            .load(package, name, data, script, hooks);
        if let Some(view) = world.get_non_send::<View>() {
            view.add_modifier(world.resource::<ModifierBook>().get(id).info());
        }
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            ctx.frame().add_modifier_params(id, &data.params);
        }
    }

    /// The modifier `name` of `package`, as `load_modifier` loaded it.
    pub fn modifier(world: &World, package: u16, name: &str) -> Option<ModifierId> {
        world.resource::<ModifierBook>().find(package, name)
    }
}

/// The hooks of a modifier's script that combat events call.
const MODIFIER_HOOKS: [Hook; 6] = [
    Hook::OnAttack,
    Hook::OnInterval,
    Hook::OnAttackHit,
    Hook::OnDamageTaken,
    Hook::OnKill,
    Hook::OnTakedown,
];

/// Ends, as each tick starts, the modifiers and stacks that hold no longer.
fn expire_modifiers(tick: Res<'_, SimTick>, mut units: Query<'_, '_, &mut Modifiers>) {
    let now = tick.start();
    for mut modifiers in &mut units {
        if modifiers.bypass_change_detection().expire(now) {
            modifiers.set_changed();
        }
    }
}

/// Ends the modifiers of each unit that died this tick, all but its passives.
fn clear_dead_modifiers(mut dead: Query<'_, '_, &mut Modifiers, Added<Dead>>) {
    for mut modifiers in &mut dead {
        modifiers.clear_on_death();
    }
}

/// Holds each aura's modifier, in Resolve each tick, on every living unit within its radius on
/// the ground plane that its `affects` selects, from the unit that carries the aura, and ends
/// it on each unit that left. The aura's modifier resolves its numbers from the ability that
/// gave the aura, and has no duration.
fn apply_auras(
    (book, stats, tick): (
        Option<Res<'_, ModifierBook>>,
        Option<Res<'_, StatBook>>,
        Res<'_, SimTick>,
    ),
    view: Option<NonSend<'_, View>>,
    abilities: Option<NonSend<'_, Ctx>>,
    mut units: Query<
        '_,
        '_,
        (
            &StableId,
            &Position,
            &Team,
            Option<&UnitType>,
            &mut Modifiers,
        ),
        Without<Dead>,
    >,
    mut held: Local<'_, Vec<Held>>,
) {
    let (Some(book), Some(stats), Some(view)) = (book, stats, view) else {
        return;
    };
    held.clear();
    for (&source, &at, &team, _, modifiers) in &units {
        for instance in modifiers.iter() {
            let (Some(aura), Some(radius)) =
                (&book.get(instance.id).data.aura, instance.aura_radius)
            else {
                continue;
            };
            let filter = view
                .resolve_filter(&aura.affects)
                .expect("the load checked the aura's filter");
            let package = book.get(instance.id).package;
            let modifier = book
                .find(package, &aura.modifier)
                .expect("the load checked the aura's modifier");
            for (&target, &pos, &other, unit_type, _) in &units {
                let tags = view.type_tags(unit_type.copied());
                if at.within_ground(pos, radius) && filter.selects(team, other, tags) {
                    held.push(Held {
                        target,
                        modifier,
                        source,
                        ability: instance.ability,
                        rank: instance.rank,
                    });
                }
            }
        }
    }
    held.sort_unstable();
    let frame = abilities.as_ref().map(|ctx| ctx.frame());
    for (&id, _, _, _, mut modifiers) in &mut units {
        let first = held.partition_point(|entry| entry.target < id);
        let mine = held[first..].iter().take_while(|entry| entry.target == id);
        let kept = |modifier, source| {
            mine.clone()
                .any(|entry| entry.modifier == modifier && Some(entry.source) == source)
        };
        if modifiers.bypass_change_detection().release_auras(kept) {
            modifiers.set_changed();
        }
        for entry in mine {
            if modifiers.get(entry.modifier, Some(entry.source)).is_some() {
                continue;
            }
            let applier = Applier {
                source: Some(entry.source),
                ability: entry.ability,
                rank: entry.rank,
                passive: false,
                aura: true,
            };
            let param = |name: &str| {
                let ability = entry.ability?;
                frame.as_ref()?.ability_param(ability, entry.rank, name)
            };
            let Some(application) =
                book.application(entry.modifier, applier, None, tick.start(), &stats, param)
            else {
                continue;
            };
            modifiers.apply(application);
        }
    }
}

/// An aura's modifier a unit holds: the unit, the modifier, the aura's carrier, and the ability
/// that gave the aura, at its rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Held {
    target: StableId,
    modifier: ModifierId,
    source: StableId,
    ability: Option<AbilityId>,
    rank: u8,
}

/// Fills a row of the script view with the unit's level, stats and modifiers.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.level = unit.get::<Level>().map(|level| level.get());
    if let Some(stats) = unit.get::<UnitStats>() {
        fill.stated(stats.values());
    }
    for instance in unit
        .get::<Modifiers>()
        .into_iter()
        .flat_map(Modifiers::iter)
    {
        fill.modified(
            instance.id,
            instance.source,
            instance.stacks,
            &instance.state,
        );
    }
}

/// Derives the stats of each unit whose level changed or that is new, and sets what holds their
/// effect: how far it walks a tick, its attack's damage and period, and its pools' maxima, a
/// pool keeping the rule of stats.md. An effect whose stat the mode does not declare keeps what
/// the unit's kit gave it.
fn refresh_stats(
    book: Option<Res<'_, StatBook>>,
    mut units: Query<
        '_,
        '_,
        (
            &UnitType,
            &Level,
            Option<&Modifiers>,
            &mut UnitStats,
            Option<&mut MoveStep>,
            Option<&mut AttackStats>,
            Option<&mut Health>,
            Option<&mut ResourcePool>,
        ),
        Or<(Changed<Level>, Changed<Modifiers>, Added<UnitStats>)>,
    >,
) {
    let Some(book) = book else {
        return;
    };
    for (&unit_type, level, modifiers, mut stats, step, attack, health, pool) in &mut units {
        book.compute(unit_type, level.get(), modifiers, stats.refill());
        let values = stats.values();
        if let (Some(mut step), Some(value)) = (step, book.step(values)) {
            step.set_if_neq(MoveStep::new(value).expect("a step is at least 0"));
        }
        if let Some(mut attack) = attack {
            let damage = book.engine(values, EngineStat::AttackDamage);
            let period = book.period(values, attack.windup());
            if let (Some(damage), Some(period)) = (damage, period) {
                let derived = attack.derived(damage.max(Num::ZERO), period);
                attack.set_if_neq(derived);
            }
        }
        let positive = |value: Num| value.max(Num::EPSILON);
        if let (Some(mut health), Some(max)) = (health, book.engine(values, EngineStat::Health)) {
            let mut changed = *health;
            changed.set_max(positive(max));
            health.set_if_neq(changed);
        }
        if let (Some(mut pool), Some(max)) = (pool, book.engine(values, EngineStat::Resource)) {
            let mut changed = *pool;
            changed.set_max(positive(max));
            pool.set_if_neq(changed);
        }
    }
}

/// Adds each living unit's regen to its pools: `health_regen` and `resource_regen` a second,
/// the tick rate's share a tick, the remainder carried so a second gains exactly the regen.
fn regenerate(
    book: Option<Res<'_, StatBook>>,
    mut units: Query<
        '_,
        '_,
        (&UnitStats, Option<&mut Health>, Option<&mut ResourcePool>),
        Without<Dead>,
    >,
) {
    let Some(book) = book else {
        return;
    };
    let hz = book.rate().hz().get();
    for (stats, health, pool) in &mut units {
        let values = stats.values();
        if let (Some(mut health), Some(regen)) =
            (health, book.engine(values, EngineStat::HealthRegen))
        {
            let mut changed = *health;
            changed.regen(regen, hz);
            health.set_if_neq(changed);
        }
        if let (Some(mut pool), Some(regen)) =
            (pool, book.engine(values, EngineStat::ResourceRegen))
        {
            let mut changed = *pool;
            changed.regen(regen, hz);
            pool.set_if_neq(changed);
        }
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{StableId, TickRate};

    use crate::abilities::ability_book::AbilityId;
    use crate::scripts::ctx::Ctx;
    use crate::stats::Stats;
    use crate::stats::modifier_book::{Applier, ModifierId};
    use crate::stats::modifier_effect::ModifierEffect;
    use crate::stats::stat::Stat;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_rule::StatRule;

    /// Gives a match with no mode the stat book of `rules`, at `rate`, with no unit type.
    pub fn load_stats(world: &mut World, rules: &BTreeMap<Stat, StatRule>, rate: TickRate) {
        let book = StatBook::new(rules, [], rate, Num::MAX).expect("rules of no type's value");
        Stats::load(world, book);
    }

    /// Gives `target` the modifier `id` from `source`, by `ability` at `rank`, and a passive when
    /// `passive`, as an application in the running tick would.
    pub fn give_modifier(
        world: &mut World,
        target: StableId,
        id: ModifierId,
        from: Option<(StableId, Option<AbilityId>, u8)>,
        passive: bool,
    ) {
        let (source, ability, rank) = from.map_or((None, None, 1), |(source, ability, rank)| {
            (Some(source), ability, rank)
        });
        let applier = Applier {
            source,
            ability,
            rank,
            passive,
            aura: false,
        };
        let ctx = world.get_non_send::<Ctx>().cloned();
        let param = |name: &str| ctx.as_ref()?.frame().ability_param(ability?, rank, name);
        let add = ModifierEffect::Add {
            target,
            id,
            duration: None,
        };
        Stats::apply_effect(world, add, applier, param);
    }
}

#[cfg(test)]
mod tests;
