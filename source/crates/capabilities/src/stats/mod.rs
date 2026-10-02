//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Changed, Has, Or, With, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, Tick, Ticks};

use crate::combat::CombatSet;
use crate::combat::combat_events::CombatEvents;
use crate::combat::dead::Dead;
use crate::navigation::move_step::MoveStep;
use crate::scripts::ctx::Ctx;
use crate::scripts::frame::Frame;
use crate::stats::held_modifiers::{Held, HeldModifiers};
use crate::stats::level::Level;
use crate::stats::live_shares::LiveShares;
use crate::stats::modifier_book::{Applier, ModifierBook, ModifierId};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifier_hooks::ModifierHooks;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::param_sources::ParamSources;
use crate::stats::player_modifiers::{PlayerModifier, PlayerModifiers};
use crate::stats::pool_book::PoolBook;
use crate::stats::pools::Pools;
use crate::stats::refresh_scratch::{RefreshScratch, Refreshing};
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::script_view::{RowFill, View};
use crate::units::tag_book::TagBook;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;
use crate::values::metric::Metric;

pub(crate) mod error;
pub(crate) mod held_modifiers;
pub(crate) mod level;
pub(crate) mod live_param;
pub(crate) mod live_shares;
pub(crate) mod modifier_book;
pub(crate) mod modifier_data;
pub(crate) mod modifier_effect;
pub(crate) mod modifier_handle;
pub(crate) mod modifier_hooks;
pub(crate) mod modifier_spec;
pub(crate) mod modifiers;
pub(crate) mod param_book;
pub(crate) mod param_read;
pub(crate) mod param_source;
pub(crate) mod param_sources;
pub(crate) mod param_table;
pub(crate) mod player_modifiers;
pub(crate) mod pool_book;
pub(crate) mod pool_cost;
pub(crate) mod pool_data;
pub(crate) mod pool_id;
pub(crate) mod pools;
pub(crate) mod refresh_scratch;
pub(crate) mod stat;
pub(crate) mod stat_book;
pub(crate) mod stat_change;
pub(crate) mod stat_graph;
pub(crate) mod stat_id;
pub(crate) mod stat_op;
pub(crate) mod stat_rule;
pub(crate) mod stat_totals;
pub(crate) mod stats_api;
pub(crate) mod stats_data;
pub(crate) mod unit_stats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

/// The systems of `stats`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StatsSet {
    /// In `SimSet::Inputs`: the modifiers and stacks that hold no longer end.
    Expire,
    /// In `SimSet::Inputs`: the living units' pools regenerate.
    Regenerate,
    /// In `SimSet::Resolve`, after `CombatSet::Die`: auras, players and `HeldModifiers` hold
    /// their modifiers.
    Hold,
}

impl Stats {
    /// Adds stats to a match: before the first stage and after each, every unit whose level or
    /// modifiers changed, or that is new, has its stats and states derived again, and the
    /// components that hold their effect follow them; as each tick starts, the living units'
    /// pools regenerate. With no stat book, as before a mode loads one or on a client, which
    /// loads none, nothing changes.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        if let Some(view) = world.get_non_send::<View>() {
            view.add_source(fill_row);
        }
        world.insert_resource(ModifierBook::default());
        world.insert_resource(PlayerModifiers::default());
        world.insert_resource(HeldModifiers::default());
        registry.register_resource::<PlayerModifiers>();
        if let Some(ctx) = world.get_non_send::<Ctx>().cloned() {
            let hooks = ModifierHooks::new(ctx);
            world.insert_non_send(CombatEvents::new(move |batch, event| {
                hooks.hear(batch, event);
            }));
        }
        registry.register_component::<Level>();
        registry.register_component::<Modifiers>();
        registry.register_component::<Pools>();
        schedule.add_systems((
            (
                expire_modifiers.in_set(StatsSet::Expire),
                regenerate.in_set(StatsSet::Regenerate),
            )
                .in_set(SimSet::Inputs),
            (clear_dead_modifiers, apply_held.in_set(StatsSet::Hold))
                .chain()
                .in_set(SimSet::Resolve)
                .after(CombatSet::Die),
            (give_derived_parts, refresh_stats)
                .chain()
                .before(SimSet::Inputs),
        ));
        for pair in SimSet::ALL.windows(2) {
            schedule.add_systems(
                (give_derived_parts, refresh_stats)
                    .chain()
                    .after(pair[0])
                    .before(pair[1]),
            );
        }
        schedule.add_systems(
            (give_derived_parts, refresh_stats)
                .chain()
                .after(SimSet::Vision),
        );
    }

    /// Gives the match the mode's stat book and pool book.
    pub(crate) fn load(world: &mut World, book: StatBook, pools: PoolBook) {
        world.insert_resource(book);
        world.insert_resource(pools);
    }

    /// Applies the next modifier effect the call in `frame` queued, from its acting unit and its
    /// ability at its rank.
    pub(crate) fn apply_next(world: &mut World, frame: &mut Frame, _: Tick) {
        let effect = frame.effects.take::<ModifierEffect>();
        let applier = Applier {
            source: frame.acting(),
            ability: frame.action(),
            rank: frame.rank(),
            passive: false,
            held: false,
        };
        Stats::apply_effect(world, effect, applier);
    }

    /// Applies `effect`, which a call by `applier` queued. An added modifier's numbers resolve
    /// now, its params read from the param book, of its source as it is now; nothing is added to a
    /// dead or gone unit, one that carries no modifiers, or when a number does not resolve.
    pub(crate) fn apply_effect(world: &mut World, effect: ModifierEffect, applier: Applier) {
        match effect {
            ModifierEffect::Add {
                target,
                id,
                duration,
            } => Stats::add_modifier(world, target, id, applier, duration),
            ModifierEffect::AddPlayer { player, id } => {
                let held = PlayerModifier {
                    player,
                    modifier: id,
                };
                world.resource_mut::<PlayerModifiers>().add(held);
            }
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
        let source = applier
            .source
            .and_then(|source| ParamSource::of(world, source));
        let params = world.resource::<ParamBook>();
        let param = |place: &ParamPlace| {
            let (ability, rank) = (applier.ability, applier.rank);
            params.modifier_param(id, ability, rank, place, source.as_ref())
        };
        let rate = stats.rate();
        let Some(application) = book.application(id, applier, duration, now, rate, param) else {
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
}

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

/// Holds, in Resolve each tick, each aura's modifier on every living unit within its radius in
/// the map's metric that its `affects` selects, from the unit that carries the aura; each one
/// another capability holds this tick, as `HeldModifiers` lists it; and each
/// player modifier on every living unit of its player that the modifier's `affects` selects,
/// from no source. Each ends on a unit that left it. An aura's modifier resolves its numbers from
/// the ability that gave the aura, a player modifier's at rank 1; neither has a duration.
fn apply_held(
    (book, stats, tick, metric, players, others): (
        Option<Res<'_, ModifierBook>>,
        Option<Res<'_, StatBook>>,
        Res<'_, SimTick>,
        Res<'_, Metric>,
        Res<'_, PlayerModifiers>,
        Res<'_, HeldModifiers>,
    ),
    params: Res<'_, ParamBook>,
    sources: ParamSources<'_, '_>,
    relations: Res<'_, Relations>,
    mut units: Query<
        '_,
        '_,
        (
            &StableId,
            &Position,
            &Team,
            Option<&UnitTags>,
            Option<&Owner>,
            &mut Modifiers,
        ),
        Without<Dead>,
    >,
    mut held: Local<'_, Vec<Held>>,
) {
    let (Some(book), Some(stats)) = (book, stats) else {
        return;
    };
    let rate = stats.rate();
    held.clear();
    held.extend_from_slice(&others.0);
    for (&target, _, _, tags, owner, _) in &units {
        let Some(owner) = owner else {
            continue;
        };
        let tags = tags.map_or(TagSet::default(), |tags| tags.tags);
        for modifier in players.of(owner.slot()) {
            let affects = book.get(modifier).spec.affects;
            if affects.is_none_or(|filter| filter.selects(Attitude::Friendly, tags)) {
                held.push(Held {
                    target,
                    modifier,
                    source: None,
                    ability: None,
                    rank: 1,
                });
            }
        }
    }
    for (&source, &at, &team, _, _, modifiers) in &units {
        for instance in modifiers.iter() {
            let (Some(aura), Some(radius)) =
                (&book.get(instance.id).spec.aura, instance.aura_radius)
            else {
                continue;
            };
            let (filter, modifier) = (aura.affects, aura.modifier);
            for (&target, &pos, &other, tags, _, _) in &units {
                let tags = tags.map_or(TagSet::default(), |tags| tags.tags);
                let attitude = relations.between(team, other);
                if metric.within(at, pos, radius) && filter.selects(attitude, tags) {
                    held.push(Held {
                        target,
                        modifier,
                        source: Some(source),
                        ability: instance.ability,
                        rank: instance.rank,
                    });
                }
            }
        }
    }
    held.sort_unstable();
    for (&id, _, _, _, _, mut modifiers) in &mut units {
        let first = held.partition_point(|entry| entry.target < id);
        let mine = held[first..].iter().take_while(|entry| entry.target == id);
        let kept = |modifier, source| {
            mine.clone()
                .any(|entry| entry.modifier == modifier && entry.source == source)
        };
        if modifiers.bypass_change_detection().release_held(kept) {
            modifiers.set_changed();
        }
        for entry in mine {
            if modifiers.get(entry.modifier, entry.source).is_some() {
                continue;
            }
            let applier = Applier {
                source: entry.source,
                ability: entry.ability,
                rank: entry.rank,
                passive: false,
                held: true,
            };
            let source = entry.source.and_then(|source| sources.get(source));
            let param = |place: &ParamPlace| {
                let (ability, rank) = (entry.ability, entry.rank);
                params.modifier_param(entry.modifier, ability, rank, place, source.as_ref())
            };
            let Some(application) =
                book.application(entry.modifier, applier, None, tick.start(), rate, param)
            else {
                continue;
            };
            modifiers.apply(application);
        }
    }
}

/// Fills a row of the script view with the unit's level, stats and modifiers.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    fill.row.level = unit.get::<Level>().map(|level| level.get());
    fill.row.pools = unit.get::<Pools>().copied();
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

/// Gives each unit of a type that lacks them the parts its stats and tags derive into, which a
/// restore does not bring back, as they are never state; the refresh after it derives them, as
/// it does a new unit's.
fn give_derived_parts(
    book: Option<Res<'_, StatBook>>,
    mut commands: Commands<'_, '_>,
    units: Query<'_, '_, Entity, (With<UnitType>, Without<UnitStats>)>,
) {
    if book.is_none() {
        return;
    }
    for unit in &units {
        commands
            .entity(unit)
            .insert((UnitStats::default(), UnitTags::default()));
    }
}

/// Derives the stats and tags of every unit whose level or modifiers changed, that is new, or
/// that carries a live change, and sets what holds their effect: how far it walks a tick and its
/// pools' maxima, a pool keeping the rule of stats.md. A
/// modifier its tags' immunities suppress gives no tags and no stats. Each stat is computed for
/// every refreshing unit in the stat book's order, so a live change reads its source's stats
/// once they are final; a live change that does not resolve keeps the value it last had. An
/// effect whose stat the mode does not declare keeps what the unit's kit gave it.
fn refresh_stats(
    (book, pool_book, tag_book, index): (
        Option<Res<'_, StatBook>>,
        Option<Res<'_, PoolBook>>,
        Option<Res<'_, TagBook>>,
        Res<'_, EntityIndex>,
    ),
    params: Res<'_, ParamBook>,
    mut commands: Commands<'_, '_>,
    mut units: ParamSet<
        '_,
        '_,
        (
            Query<
                '_,
                '_,
                (
                    Entity,
                    &StableId,
                    &UnitType,
                    &Level,
                    Option<&Modifiers>,
                    Option<&mut UnitTags>,
                    Has<LiveShares>,
                ),
                (
                    With<UnitStats>,
                    Or<(
                        Changed<Level>,
                        Changed<Modifiers>,
                        Added<UnitStats>,
                        With<LiveShares>,
                    )>,
                ),
            >,
            Query<'_, '_, (&UnitType, &Level, &UnitStats)>,
            Query<'_, '_, (&mut UnitStats, Option<&mut MoveStep>, Option<&mut Pools>)>,
        ),
    >,
    mut scratch: Local<'_, RefreshScratch>,
) {
    let (Some(book), Some(pool_book)) = (book, pool_book) else {
        return;
    };
    let scratch = &mut *scratch;
    scratch.clear();
    let granting = tag_book
        .as_deref()
        .map_or(TagSet::default(), TagBook::granting);
    for (entity, &id, &unit_type, level, modifiers, tags, marked) in &mut units.p0() {
        let held = modifiers.into_iter().flat_map(Modifiers::iter);
        let granted = held
            .filter(|instance| instance.stacks > 0)
            .map(|instance| instance.tags);
        let derived = tag_book
            .as_deref()
            .map(|book| book.unit_tags(unit_type, granted));
        if let (Some(mut tags), Some(derived)) = (tags, derived) {
            tags.set_if_neq(derived);
        }
        let immune = derived.map_or(TagSet::default(), |derived| derived.immune);
        let takes_effect = TagBook::effect_test(granting, immune);
        let unit = Refreshing {
            id,
            entity,
            unit_type,
            level: level.get(),
        };
        let live = scratch.add(&book, unit, modifiers, takes_effect);
        if live && !marked {
            commands.entity(entity).insert(LiveShares);
        } else if !live && marked {
            commands.entity(entity).remove::<LiveShares>();
        }
    }
    if scratch.units.is_empty() {
        return;
    }
    let sources = units.p1();
    let other = |id| {
        let (&unit_type, level, stats) = sources.get(index.get(id)?).ok()?;
        Some(ParamSource::new(
            &book,
            unit_type,
            level.get(),
            stats.values(),
        ))
    };
    scratch.compute(&book, &params, other);
    let count = usize::from(book.len());
    let mut writes = units.p2();
    for (unit, refreshing) in scratch.units.iter().enumerate() {
        let Ok((mut stats, step, pools)) = writes.get_mut(refreshing.entity) else {
            continue;
        };
        let values = scratch.values(unit, count);
        let refill = stats.refill();
        refill.extend_from_slice(values);
        if let (Some(mut step), Some(value)) = (step, book.step(values)) {
            step.set_if_neq(MoveStep::new(value).expect("a step is at least 0"));
        }
        if let Some(mut pools) = pools {
            let mut changed = *pools;
            for (pool, stats) in pool_book.iter() {
                let max = values[stats.max.index()].max(Num::EPSILON);
                changed.set_max(pool, max);
            }
            pools.set_if_neq(changed);
        }
    }
}

/// Adds each living unit's regen to its pools: each pool's `regen` stat a second, the tick
/// rate's share a tick, the remainder carried so a second gains exactly the regen.
fn regenerate(
    (book, pool_book): (Option<Res<'_, StatBook>>, Option<Res<'_, PoolBook>>),
    mut units: Query<'_, '_, (&UnitStats, &mut Pools), Without<Dead>>,
) {
    let (Some(book), Some(pool_book)) = (book, pool_book) else {
        return;
    };
    let hz = book.rate().hz().get();
    for (stats, mut pools) in &mut units {
        let values = stats.values();
        let mut changed = *pools;
        for (pool, stats) in pool_book.iter() {
            if let Some(regen) = stats.regen {
                changed.regen(pool, values[regen.index()], hz);
            }
        }
        pools.set_if_neq(changed);
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{EntityIndex, StableId, TickRate};

    use crate::actions::action_book::ActionId;
    use crate::stats::Stats;
    use crate::stats::modifier_book::{Applier, ModifierId};
    use crate::stats::modifier_effect::ModifierEffect;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_book::PoolBook;
    use crate::stats::stat::Stat;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_rule::StatRule;
    use bevy_ecs::world::Mut;
    use campfire_script::ScriptId;

    use crate::scripts::script_book::ScriptBook;
    use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::stats::modifier_data::ModifierData;
    use crate::units::script_view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;

    impl Stats {
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
            let name = DeclaredName::new(name).expect("a modifier's name is a name");
            world.resource_scope(|world, mut book: Mut<'_, ModifierBook>| {
                let scripts = world.resource::<ScriptBook>();
                let view = world.non_send::<View>();
                let places = StatBook::places(world, data.stats.keys());
                let mut types = view.types_mut();
                let load = ModifierLoad {
                    scripts,
                    types: &mut types,
                    stat: |stat: &Stat| places[stat],
                    rate: *world.resource::<TickRate>(),
                };
                let modifier = PackageModifier {
                    name: &name,
                    data,
                    script,
                };
                book.load(load, package, &[modifier])
                    .expect("a test's modifier loads");
            });
            let book = world.resource::<ModifierBook>();
            let id = book
                .find(package, name.as_str())
                .expect("the modifier loaded");
            world.non_send::<View>().set_modifiers(book.clone());
            let places = StatBook::places(world, data.params.values().flat_map(Param::stats));
            ParamBook::load_modifier(world, id, &data.params, |stat| places[stat]);
        }

        /// The modifier `name` of `package`, as `load_modifier` loaded it.
        pub fn modifier(world: &World, package: u16, name: &str) -> Option<ModifierId> {
            world.resource::<ModifierBook>().find(package, name)
        }
    }

    /// Gives a match with no mode the stat book of `rules`, at `rate`, with no unit type and no
    /// pool.
    pub fn load_stats(world: &mut World, rules: &BTreeMap<Stat, StatRule>, rate: TickRate) {
        let book = StatBook::new(rules, [], rate, Num::MAX);
        Stats::load(world, book, PoolBook::default());
    }

    /// Gives `target` the modifier `id` from `source`, by `ability` at `rank`, and a passive when
    /// `passive`, as an application in the running tick would.
    pub fn give_modifier(
        world: &mut World,
        target: StableId,
        id: ModifierId,
        from: Option<(StableId, Option<ActionId>, u8)>,
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
            held: false,
        };
        let add = ModifierEffect::Add {
            target,
            id,
            duration: None,
        };
        Stats::apply_effect(world, add, applier);
    }

    /// The modifiers `unit` carries, each with its source, in their order.
    pub fn carried(world: &World, unit: StableId) -> Vec<(ModifierId, Option<StableId>)> {
        let entity = world
            .resource::<EntityIndex>()
            .get(unit)
            .expect("a unit of the match");
        world
            .get::<Modifiers>(entity)
            .into_iter()
            .flat_map(Modifiers::iter)
            .map(|instance| (instance.id, instance.source))
            .collect()
    }
}

#[cfg(test)]
mod tests;
