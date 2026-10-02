//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::query::Added;
use bevy_ecs::query::ROQueryItem;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_sim::{EntityIndex, SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::scripts::ctx::Ctx;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::held_modifiers::HeldModifiers;
use crate::stats::held_pass::HeldPass;
use crate::stats::level::Level;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::player_modifiers::{PlayerModifier, PlayerModifiers};
use crate::stats::pool_book::PoolBook;
use crate::stats::pools::Pools;
use crate::stats::refresh::Refresh;
use crate::stats::stat_book::StatBook;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::StatsColumn;
use crate::stats::unit_stats::UnitStats;
use crate::units::dead::Dead;
use crate::units::modifier_id::ModifierId;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;

pub(crate) mod application;
pub(crate) mod applier;
pub(crate) mod carried_mut;
pub(crate) mod error;
pub(crate) mod held_modifiers;
pub(crate) mod held_pass;
pub(crate) mod instance;
pub(crate) mod level;
pub(crate) mod life_pool;
pub(crate) mod lifetime;
pub(crate) mod live_param;
pub(crate) mod live_shares;
pub(crate) mod meter;
pub(crate) mod modifier_book;
pub(crate) mod modifier_clocks;
pub(crate) mod modifier_data;
pub(crate) mod modifier_effect;
pub(crate) mod modifier_handle;
pub(crate) mod modifier_spec;
pub(crate) mod modifiers;
pub(crate) mod move_step;
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
pub(crate) mod refresh;
pub(crate) mod refresh_scratch;
pub(crate) mod stat_book;
pub(crate) mod stat_change;
pub(crate) mod stat_graph;
pub(crate) mod stat_id;
pub(crate) mod stat_op;
pub(crate) mod stat_rule;
pub(crate) mod stat_totals;
pub(crate) mod stats_api;
pub(crate) mod stats_call;
pub(crate) mod stats_column;
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
    /// In `SimSet::Resolve`, after the tick's deaths: the dead units' modifiers end, then
    /// auras, players and `HeldModifiers` hold their modifiers.
    Hold,
}

impl Stats {
    /// Adds stats to a match: before the first stage and after each, every unit whose level or
    /// modifiers changed, or that is new, has its stats and states derived again, and the
    /// components that hold their effect follow them; as each tick starts, the living units'
    /// pools regenerate. With no stat book, as before a mode loads one or on a client, which
    /// loads none, nothing changes.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(StatsColumn::default());
        view.add_source::<RowParts>(world, fill_row);
        world.insert_resource(ModifierBook::default());
        world.insert_resource(ParamBook::default());
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            ctx.frame().add_part(StatsCall::default());
        }
        world.insert_resource(PlayerModifiers::default());
        world.insert_resource(HeldModifiers::default());
        registry.register_resource::<PlayerModifiers>();
        registry.register_component::<Level>();
        registry.register_component::<Modifiers>();
        registry.register_component::<ModifierClocks>();
        registry.register_component::<Pools>();
        schedule.add_systems((
            (
                expire_modifiers.in_set(StatsSet::Expire),
                Refresh::regenerate.in_set(StatsSet::Regenerate),
            )
                .in_set(SimSet::Inputs),
            (clear_dead_modifiers, HeldPass::run)
                .chain()
                .in_set(SimSet::Resolve)
                .in_set(StatsSet::Hold),
            (Refresh::give_parts, Refresh::run)
                .chain()
                .before(SimSet::Inputs),
        ));
        for pair in SimSet::ALL.windows(2) {
            schedule.add_systems(
                (Refresh::give_parts, Refresh::run)
                    .chain()
                    .after(pair[0])
                    .before(pair[1]),
            );
        }
        schedule.add_systems(
            (Refresh::give_parts, Refresh::run)
                .chain()
                .after(SimSet::Vision),
        );
    }

    /// Gives the match the mode's stat book and pool book, whose names its scripts read.
    pub(crate) fn load(world: &mut World, book: StatBook, pools: PoolBook) {
        let view = world.non_send::<View>();
        StatsColumn::share_stat_names(view, book.names());
        StatsColumn::share_pool_names(view, pools.names());
        world.insert_resource(book);
        world.insert_resource(pools);
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
        let rate = *world.resource::<TickRate>();
        if !world.contains_resource::<StatBook>() {
            return;
        }
        let Some(book) = world.get_resource::<ModifierBook>() else {
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
        let Some(application) = book.application(id, applier, duration, now, rate, param) else {
            return;
        };
        if let Some(mut carried) = CarriedMut::of(world, entity) {
            carried.apply(application);
        }
    }

    fn remove_modifier(
        world: &mut World,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) {
        let entity = world.resource::<EntityIndex>().get(carrier);
        if let Some(mut carried) = entity.and_then(|entity| CarriedMut::of(world, entity)) {
            carried.remove(id, source);
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
        if let Some(mut carried) = entity.and_then(|entity| CarriedMut::of(world, entity)) {
            carried.write(data.id, data.source, data.stacks, &data.state, now);
        }
    }
}

/// Ends, as each tick starts, the modifiers and stacks that hold no longer.
fn expire_modifiers(
    tick: Res<'_, SimTick>,
    mut units: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks)>,
) {
    let now = tick.start();
    for (modifiers, clocks) in &mut units {
        CarriedMut::new(modifiers, clocks).expire(now);
    }
}

/// Ends the modifiers of each unit that died this tick, all but its passives.
fn clear_dead_modifiers(
    mut dead: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks), Added<Dead>>,
) {
    for (modifiers, clocks) in &mut dead {
        CarriedMut::new(modifiers, clocks).clear_on_death();
    }
}

/// The parts of a unit the stats read into its row.
type RowParts = (
    Option<&'static Level>,
    Option<&'static Pools>,
    Option<&'static UnitStats>,
    Option<&'static Modifiers>,
    Option<&'static ModifierClocks>,
);

/// Adds the unit's level, pools, stats and modifiers to the stats' column of the script view.
fn fill_row(parts: ROQueryItem<'_, '_, RowParts>, fill: &mut RowFill<'_>) {
    let (level, pools, stats, modifiers, clocks) = parts;
    let stats = stats.map_or(&[][..], UnitStats::values);
    fill.column::<StatsColumn>().push(
        level.map(|level| level.get()),
        pools.copied(),
        stats,
        modifiers,
        clocks,
    );
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_sim::{EntityIndex, StableId};

    use crate::stats::Stats;
    use crate::stats::applier::Applier;
    use crate::stats::lifetime::Hold;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::modifier_effect::ModifierEffect;
    use crate::stats::modifiers::Modifiers;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;

    impl Stats {
        /// The modifier `name` of `package`, as the match loaded it.
        pub fn modifier(world: &World, package: u16, name: &str) -> Option<ModifierId> {
            world.resource::<ModifierBook>().named(package, name)
        }
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
            hold: passive.then_some(Hold::Passive),
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
pub(crate) mod loads {
    use std::collections::BTreeMap;

    use bevy_ecs::world::{Mut, World};
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::scripts::script_book::ScriptBook;
    use crate::stats::Stats;
    use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::stats::modifier_data::ModifierData;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_book::PoolBook;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_rule::StatRule;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::script_view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    impl Stats {
        /// Loads `data` as the modifier `name` of `package`: 0 the mode, then each package it
        /// depends on, in its manifest's order, with its compiled `script` exactly when its data
        /// names one. Modifiers load by package, then name.
        pub(crate) fn load_modifier(
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
                .named(package, name.as_str())
                .expect("the modifier loaded");
            StatsColumn::share_modifiers(world.non_send::<View>(), book.clone());
            let places = StatBook::places(world, data.params.values().flat_map(Param::stats));
            ParamBook::load_modifier(world, id, &data.params, |stat| places[stat]);
        }

        /// Gives a match with no mode the stat book `book`, with no pool stats; the pools its
        /// scripts name stay named.
        pub(crate) fn load_book(world: &mut World, book: StatBook) {
            StatsColumn::share_stat_names(world.non_send::<View>(), book.names());
            world.insert_resource(book);
            world.insert_resource(PoolBook::default());
        }
    }

    /// Gives a match with no mode the stat book of `rules`, with no unit type and no pool stats;
    /// the pools its scripts name stay named.
    pub(crate) fn load_stats(world: &mut World, rules: &BTreeMap<Stat, StatRule>) {
        Stats::load_book(world, StatBook::new(rules, [], Num::MAX));
    }
}

#[cfg(test)]
mod tests;
