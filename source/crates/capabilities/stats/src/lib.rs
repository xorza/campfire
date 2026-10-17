//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_declare::{
    ByType, ClientForm, Declarations, InstallCx, Module, ModuleSet, ServerForm, SetPlace,
    StateFields,
};
use campfire_math::Num;
use campfire_players::Player;
use campfire_sim::{Capability, SimEdge, SimSet, SimTick, SimWorld, StableId, TickRate};
use campfire_units::{
    BodyUnit, Dead, DeclaredTags, Due, Owner, PoolId, SpawnHooks, UnitGrid, UnitSteps, UnitsSet,
};
use campfire_values::{ActionId, ModifierId, Rank, Tag};

use crate::application::{Adding, ApplyAt};
use crate::aura_carriers::AuraCarriers;
use crate::modifier_adds::ModifierAdd;
use crate::player_grants::PlayerGrants;
use crate::stats_column::RowReads;

use crate::modifier_intervals::IntervalDue;

mod application;
mod applier;
mod applier_param;
mod aura_carriers;
mod carried_mut;
mod error;
mod grant_pass;
mod grants;
mod held_modifiers;
mod held_pass;
mod instance;
mod interval;
mod interval_revival;
mod level;
mod life;
mod life_pool;
mod lifetime;
mod live_carriers;
mod live_param;
mod meter;
mod mode_pools_data;
mod mode_stats_data;
mod modifier_adds;
mod modifier_book;
mod modifier_data;
mod modifier_ends;
mod modifier_events;
mod modifier_handle;
mod modifier_hooks;
mod modifier_intervals;
mod modifier_param_table;
mod modifier_params;
mod modifier_spec;
mod modifiers;
mod modifiers_data;
mod move_speed_cap;
mod param_book;
mod param_read;
mod param_source;
mod param_table;
mod passive_data;
mod player_grants;
mod player_modifiers;
mod pool_book;
mod pool_data;
mod pool_names;
mod pools;
mod refresh;
mod refresh_scratch;
mod stat_book;
mod stat_books;
mod stat_change;
mod stat_graph;
mod stat_id;
mod stat_op;
mod stat_rule;
mod stat_totals;
mod stats_api;
mod stats_call;
mod stats_column;
mod stats_data;
mod stats_effect;
mod stats_world;
mod type_passive;
mod type_pools;
mod type_pools_data;
mod unit_pools;
mod unit_stats;

pub use crate::application::{Application, NewInstance};
pub use crate::applier::Applier;
pub use crate::applier_param::ApplierParam;
pub use crate::carried_mut::CarriedMut;
pub use crate::error::{
    EdgeError, ModifierError, ModifierProblem, ModifiersDataError, SpecError, StatGraphError,
    StatsRefusal, UnitStartError,
};
pub use crate::grant_pass::GrantPass;
pub use crate::grants::{Grant, GrantChange, GrantFrom, Grants, GrantsIter, UnitGrants};
pub use crate::held_modifiers::{Held, HeldModifiers};
pub use crate::held_pass::HeldPass;
pub use crate::instance::{Instance, StackEnd, StatShare};
pub use crate::interval::Interval;
pub use crate::interval_revival::IntervalRevival;
pub use crate::level::Level;
pub use crate::life::{Life, lives};
pub use crate::life_pool::LifePool;
pub use crate::lifetime::{Ends, Hold, Lifetime};
pub use crate::live_carriers::LiveCarriers;
pub use crate::live_param::{LiveParam, ParamOwner};
pub use crate::meter::Meter;
pub use crate::mode_pools_data::ModePoolsData;
pub use crate::mode_stats_data::ModeStatsData;
pub use crate::modifier_book::{ModifierBook, ModifierEntry, ModifierLoad, PackageModifier};
pub use crate::modifier_data::{AuraData, ModifierData, Reapply};
pub use crate::modifier_ends::ModifierEnds;
pub use crate::modifier_events::ModifierEvents;
pub use crate::modifier_handle::{HandleData, HandleOf, ModifierHandle};
pub use crate::modifier_hooks::ModifierHooks;
pub use crate::modifier_intervals::ModifierIntervals;
pub use crate::modifier_param_table::ModifierParamTable;
pub use crate::modifier_params::ModifierParams;
pub use crate::modifier_spec::{
    AuraSpec, ModifierSpec, ParamPlace, SpecChange, SpecNames, SpecNumber, SpecTime,
};
pub use crate::modifiers::modifier_clocks::{Clock, ModifierClocks, Spent};
pub use crate::modifiers::{Absorbed, Carried, Modifiers, Touched};
pub use crate::modifiers_data::ModifiersData;
pub use crate::move_speed_cap::MoveSpeedCap;
pub use crate::param_book::{ParamBook, ParamTables};
pub use crate::param_source::ParamSource;
pub use crate::param_table::ParamTable;
pub use crate::passive_data::PassiveData;
pub use crate::player_modifiers::PlayerModifiers;
pub use crate::pool_book::{PoolBook, PoolStats};
pub use crate::pool_data::PoolData;
pub use crate::pool_names::PoolNames;
pub use crate::pools::Pools;
pub use crate::refresh::Refresh;
pub use crate::stat_book::StatBook;
pub use crate::stat_change::StatChange;
pub use crate::stat_graph::StatGraph;
pub use crate::stat_id::StatId;
pub use crate::stat_op::StatOp;
pub use crate::stat_rule::StatRule;
pub use crate::stat_totals::StatTotals;
pub use crate::stats_api::StatsApi;
pub use crate::stats_call::StatsCall;
pub use crate::stats_column::StatsColumn;
pub use crate::stats_data::{StatValue, StatsData};
pub use crate::stats_effect::StatsEffect;
pub use crate::stats_world::StatsWorld;
pub use crate::type_passive::TypePassive;
pub use crate::type_pools::TypePools;
pub use crate::type_pools_data::TypePoolsData;
pub use crate::unit_pools::{PoolMeters, PoolMetersMut, UnitPools, UnitPoolsReadOnly};
pub use crate::unit_stats::UnitStats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

/// The steps of `stats`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatsSet {
    /// The modifiers and stacks that hold no longer end.
    Expire,
    /// The living units' pools regenerate.
    Regenerate,
    /// Once the tick's orders and trades are in: the grants the sources pushed apply.
    GrantAtInputs,
    /// Once the attacks going off paid their toggles and before they strike: the grants the
    /// sources pushed apply.
    GrantAtStrike,
    /// Modifiers' intervals come.
    Interval,
    /// Once the tick's damage is dealt and before units die: the grants the sources pushed apply.
    GrantAtResolve,
    /// The grants the sources pushed apply.
    GrantAtEnd,
    /// After the tick's deaths: the dead units' modifiers end.
    ClearDead,
    /// Auras, players and `HeldModifiers` hold their modifiers.
    Hold,
    /// In its gap: every unit whose level or modifiers changed, or that is new, derives its stats
    /// and states again.
    Refresh(SimEdge),
}

impl StatsSet {
    /// The steps in the stages.
    const STEPS: [Self; 9] = [
        Self::Expire,
        Self::Regenerate,
        Self::GrantAtInputs,
        Self::GrantAtStrike,
        Self::Interval,
        Self::GrantAtResolve,
        Self::GrantAtEnd,
        Self::ClearDead,
        Self::Hold,
    ];

    /// The steps in the stages, then a refresh in each gap, `SimEdge::Start` first.
    const ALL: [Self; Self::STEPS.len() + 1 + SimSet::ALL.len()] = {
        let steps = Self::STEPS.len();
        let mut all = [Self::Refresh(SimEdge::Start); Self::STEPS.len() + 1 + SimSet::ALL.len()];
        let mut at = 0;
        while at < steps {
            all[at] = Self::STEPS[at];
            at += 1;
        }
        let mut stage = 0;
        while stage < SimSet::ALL.len() {
            all[steps + 1 + stage] = Self::Refresh(SimEdge::After(SimSet::ALL[stage]));
            stage += 1;
        }
        all
    };
}

impl ModuleSet for StatsSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let stage = SetPlace::stage;
        match self {
            Self::Expire | Self::Regenerate => SetPlace::at(UnitsSet::Begin),
            Self::GrantAtInputs => stage(SimSet::Inputs).after(UnitsSet::Begin),
            Self::GrantAtStrike | Self::Interval => stage(SimSet::Hit),
            Self::GrantAtResolve => stage(SimSet::Resolve),
            Self::ClearDead => stage(SimSet::Resolve).after(Self::GrantAtResolve),
            Self::Hold => stage(SimSet::Resolve).after(Self::ClearDead),
            Self::GrantAtEnd => stage(SimSet::Vision).before(UnitsSet::End),
            Self::Refresh(edge) => SetPlace::gap(edge),
        }
    }
}

impl Module for Stats {
    const CAPABILITY: Option<Capability> = Some(Capability::Stats);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Set = StatsSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.scratch::<Due<IntervalDue>>();
        d.section::<ModifiersData>();
        d.section::<TypePoolsData>();
        d.section::<StatsData>();
        d.section::<PassiveData>();
        d.section::<ModeStatsData>();
        d.section::<ModePoolsData>();
        d.removals::<Dead>();
        d.removals::<Modifiers>();
        d.removals::<Owner>();
        d.component::<PlayerModifiers>();
        d.component::<Level>();
        d.component::<Modifiers>();
        d.component::<ModifierClocks>();
        d.component::<Pools>();
        d.scratch::<HeldModifiers>();
        d.scratch::<Grants>();
        d.cache::<AuraCarriers>();
        d.cache::<LiveCarriers>();
        d.derived::<UnitStats>();
        d.part::<Player, PlayerModifiers>(|_| Some(PlayerModifiers::default()));
        d.non_send::<ModifierEvents>(|world, role| Some(ModifierEvents::install(world, role)));
        d.rule(ModifiersData::rule);
        d.extend::<StateFields>(ModifiersData::state_fields);
        d.extend::<DeclaredTags>(ModifiersData::granted_tags);
        d.book::<ModifierBook>(ModifierBook::build);
        d.fact::<ModifierParamTable>(ModifierParamTable::build);
        d.fact::<StatGraph>(StatGraph::build);
        d.rule(StatGraph::rule);
        d.book::<StatBook>(StatBook::build);
        d.fact::<UnitSteps>(StatBook::unit_steps);
        d.fact::<PoolNames>(PoolNames::build);
        d.rule(PoolNames::rule);
        d.book::<PoolBook>(PoolBook::build);
        d.fact::<TypePools>(TypePools::build);
        d.rule(TypePools::rule);
        d.column::<StatsColumn, RowReads>(StatsColumn::fill_row);
        d.call_params::<StatsCall>();
        d.api(StatsApi::register);
        d.book::<ByType<Pools>>(TypePools::pools);
        d.book::<ByType<TypePassive>>(TypePassive::book);
        d.callback::<SpawnHooks>(Capability::Stats, TypePassive::grant);
        d.part::<BodyUnit, Level>(|_| Some(Level::default()));
        d.observer(Level::raise);
        d.part::<BodyUnit, Modifiers>(|_| Some(Modifiers::default()));
        d.part::<BodyUnit, ModifierClocks>(|_| Some(ModifierClocks::default()));
        d.part::<BodyUnit, Pools>(|at| at.of_type::<Pools>().copied());
    }

    /// Adds stats to a match: in `SimEdge::Start` and in the `SimEdge::After` of each stage,
    /// every unit whose level or modifiers changed, or that is new, has its stats and states
    /// derived again, and the components that hold their effect follow them; as each tick
    /// starts, the living units' pools regenerate. The grants the sources push apply as each
    /// tick starts, before attacks strike, before units die and as the tick ends. With no stat
    /// book, as before a mode loads one, nothing changes. The regen and the intervals install
    /// only in a match whose pools and modifiers have them (02-engine-core.md, Books before the
    /// install).
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        let regenerates = cx.book::<PoolBook>().is_some_and(PoolBook::regenerates);
        let intervals = cx
            .book::<ModifierBook>()
            .is_some_and(ModifierBook::has_intervals);
        cx.add(StatsSet::Expire, ModifierEnds::expire_modifiers);
        if regenerates {
            cx.add(StatsSet::Regenerate, Refresh::regenerate);
        }
        for set in [
            StatsSet::GrantAtInputs,
            StatsSet::GrantAtStrike,
            StatsSet::GrantAtEnd,
        ] {
            cx.add(set, GrantPass::run);
        }
        cx.add(
            StatsSet::GrantAtResolve,
            (PlayerGrants::want, GrantPass::run).chain(),
        );
        cx.add(StatsSet::ClearDead, ModifierEnds::clear_dead_modifiers);
        if intervals {
            cx.add(StatsSet::Interval, ModifierIntervals::count_intervals);
            cx.scripted(
                StatsSet::Interval,
                ModifierIntervals::run_intervals.after(ModifierIntervals::count_intervals),
            );
        }
        cx.by_role(
            StatsSet::Hold,
            (
                AuraCarriers::track,
                UnitGrid::before_reading(HeldPass::run::<ServerForm>),
            )
                .chain(),
            (
                AuraCarriers::track,
                UnitGrid::before_reading(HeldPass::run::<ClientForm>),
            )
                .chain(),
        );
        for set in StatsSet::ALL {
            if let StatsSet::Refresh(_) = set {
                cx.add(set, Refresh::run);
            }
        }
    }
}

impl Stats {
    /// Restores `amount` of unit `id`'s `pool`, unscaled, when the unit lives: it is not dead, and
    /// its life, if it has one, is above zero, as a unit with no life lives until it dies.
    pub fn restore(world: &mut World, id: StableId, pool: PoolId, amount: Num) {
        let Some(entity) = world.entity_of(id) else {
            return;
        };
        if amount <= Num::ZERO {
            return;
        }
        let life = world.life_pool();
        let mut unit = world.entity_mut(entity);
        if let Ok(pools) = unit.get_components_mut::<UnitPools>()
            && pools.lives()
        {
            pools.of(life).add(pool, amount);
        }
    }

    /// Ends the applications of the modifiers `carrier` holds that grant `tag`: an instance no
    /// hold keeps ends, and one a passive, an aura, an area or a player holds stays, as its holder
    /// would apply it again.
    fn purge(world: &mut World, carrier: StableId, tag: Tag) {
        let Some(entity) = world.entity_of(carrier) else {
            return;
        };
        let book = world.resource::<ModifierBook>().clone();
        if let Some(mut carried) = CarriedMut::of(world, entity) {
            carried.purge(|id| book.tags(id).contains(tag));
        }
    }

    /// Adds `add` from a world alone, as `HeldModifierAdds::add` adds it, its shares and state
    /// held first: for a spawn's hook and a test, which hold no call's frame.
    fn add_modifier(world: &mut World, add: ModifierAdd) {
        let Some(entity) = world.entity_of(add.target) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let at = ApplyAt {
            now: world.resource::<SimTick>().start(),
            rate: *world.resource::<TickRate>(),
            params: world.resource::<ParamBook>(),
        };
        let source = add
            .applier
            .source
            .and_then(|source| ParamSource::of(world, source));
        let adding = Adding {
            id: add.id,
            applier: add.applier,
            source: source.as_ref(),
            duration: add.duration,
        };
        let application = world
            .resource::<ModifierBook>()
            .application(at, adding)
            .owned();
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
        let entity = world.entity_of(carrier);
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
        let entity = world.entity_of(data.carrier);
        if let Some(mut carried) = entity.and_then(|entity| CarriedMut::of(world, entity)) {
            carried.write(data.id, data.source, data.stacks, &data.state, now);
        }
    }

    /// `has_way` of the match's books at its rate.
    pub fn modifier_has_way(
        world: &World,
        id: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
    ) -> bool {
        let (book, params) = (
            world.resource::<ModifierBook>(),
            world.resource::<ParamBook>(),
        );
        book.has_way(id, action, rank, params, *world.resource::<TickRate>())
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::{Mut, World};
    use campfire_common::DeclaredName;
    use campfire_declare::Book;
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{SimWorld, StableId, TickRate};
    use campfire_units::{ScriptBook, UnitTypeBook, Units};
    use campfire_values::{
        ActionId, ModifierId, NameId, Names, PackageIndex, Param, Rank, Stat, TagSet, UnitType,
    };

    use crate::Stats;
    use crate::applier::Applier;
    use crate::life::Life;
    use crate::lifetime::Hold;
    use crate::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::modifier_data::ModifierData;
    use crate::modifiers::Modifiers;
    use crate::param_book::ParamBook;
    use crate::stat_book::StatBook;
    use crate::stat_id::StatId;
    use crate::stat_op::StatOp;
    use crate::stat_rule::StatRule;
    use crate::stats_effect::StatsEffect;

    impl Stats {
        /// The modifier `name` of `package`, as the match loaded it.
        pub fn modifier(world: &World, package: PackageIndex, name: &str) -> Option<ModifierId> {
            world
                .resource::<ModifierBook>()
                .names()
                .named((package, name))
        }

        /// Gives `target` the modifier `id` from `source`, by `action` at `rank`, and a passive when
        /// `passive`, as an application in the running tick would.
        pub fn give_modifier(
            world: &mut World,
            target: StableId,
            id: ModifierId,
            from: Option<(StableId, Option<ActionId>, u8)>,
            passive: bool,
        ) {
            let (source, action, rank) = from.map_or((None, None, 1), |(source, action, rank)| {
                (Some(source), action, rank)
            });
            let rank = Rank::new(rank).expect("a modifier is given at a rank");
            let applier = Applier {
                source,
                action,
                rank,
                hold: passive.then_some(Hold::Passive),
            };
            let add = StatsEffect::Add {
                target,
                id,
                duration: None,
            };
            add.apply_from(world, applier);
        }

        /// The modifiers `id` carries, each with its source, in their order.
        pub fn carried_modifiers(
            world: &World,
            id: StableId,
        ) -> Vec<(ModifierId, Option<StableId>)> {
            let entity = world.entity_of(id).expect("a unit of the match");
            world
                .get::<Modifiers>(entity)
                .into_iter()
                .flat_map(Modifiers::iter)
                .map(|instance| (instance.id, instance.source))
                .collect()
        }
    }

    impl Stats {
        /// The life unit `id` has left, a whole amount.
        pub fn health(world: &World, id: StableId) -> i64 {
            world
                .part::<Life>(id)
                .current()
                .to_int()
                .expect("a whole amount of life")
        }

        /// Adds to the match's books a modifier of no params, script, times or state that changes
        /// each stat of `changes` by its op, by a value its instances hold, and grants `tags`:
        /// what a test's instances are instances of. Its id, after every modifier before it.
        pub fn push_modifier_changes(
            world: &mut World,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let id = world
                .resource_mut::<ModifierBook>()
                .push_changes(changes, tags);
            Self::load_modifier_params(world, id, &BTreeMap::new(), |_| StatId::new(0));
            id
        }

        /// Loads `data` as the modifier `name` of `package`: 0 the mode, then each package it
        /// depends on, in its manifest's order, with its compiled `script` exactly when its data
        /// names one. Modifiers load by package, then name.
        pub fn load_modifier(
            world: &mut World,
            package: PackageIndex,
            name: &str,
            data: &ModifierData,
            script: Option<ScriptId>,
        ) {
            let name = DeclaredName::new(name).expect("a modifier's name is a name");
            // A test's package names no tag before its modifiers, as a mode's does.
            let tags = world.resource::<UnitTypeBook>().tags();
            let new = data
                .tags
                .iter()
                .any(|tag| tags.named(tag.as_str()).is_none());
            if new {
                Units::declare_tags(world, data.tags.iter().map(DeclaredName::as_str));
            }
            world.resource_scope(|world, mut book: Mut<'_, ModifierBook>| {
                let tags = world.resource::<UnitTypeBook>().tags();
                // A match that runs no scripts holds no script book, and its modifiers none.
                let none = ScriptBook::default();
                let scripts = world.get_resource::<ScriptBook>().unwrap_or(&none);
                let places = Self::stat_places(world, data.stats.keys());
                let load = ModifierLoad {
                    scripts,
                    tags,
                    stat: |stat: &Stat| Ok(places[stat]),
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
                .names()
                .named((package, name.as_str()))
                .expect("the modifier loaded");
            ModifierBook::share_in(world);
            let places = Self::stat_places(world, data.params.values().flat_map(Param::stats));
            Self::load_modifier_params(world, id, &data.params, |stat| places[stat]);
        }

        /// Gives a match with no mode the stat book `book`, with no pool stats; the pools its
        /// scripts name stay named.
        pub fn load_book(world: &mut World, book: StatBook) {
            book.load_into(world);
        }

        /// Gives `unit_type` `values` at level 1 in the match's stat book, by the stats' places,
        /// as its `stats` section would.
        pub fn give_type_stats(world: &mut World, unit_type: UnitType, values: &[Num]) {
            world.resource_mut::<StatBook>().add_base(unit_type, values);
        }

        /// Gives a match with no mode the stat book of `rules`, with no unit type and no pool stats;
        /// the pools its scripts name stay named.
        pub fn load_rules(world: &mut World, rules: &BTreeMap<Stat, StatRule>) {
            Self::load_book(world, StatBook::new(rules, Num::MAX));
        }

        /// Adds the params of `action`, the one the match's action book loaded last, each stat at
        /// its place `stat` gives.
        pub fn load_action_params(
            world: &mut World,
            action: ActionId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let run = world.resource_mut::<ParamBook>().push_action(params, stat);
            assert_eq!(run, action.index(), "one run of params per action");
            Self::resolve_appliers(world);
        }

        /// Adds the params of `modifier`, the one the match's modifier book loaded last, each stat
        /// at its place `stat` gives.
        pub(crate) fn load_modifier_params(
            world: &mut World,
            modifier: ModifierId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let run = world
                .resource_mut::<ParamBook>()
                .push_modifier(params, stat);
            assert_eq!(run, modifier.index(), "one run of params per modifier");
            Self::resolve_appliers(world);
        }

        /// Resolves the params the match's modifiers read from their abilities in each action's
        /// params again; a world with no modifier book has none.
        fn resolve_appliers(world: &mut World) {
            world.resource_scope(|world, mut params: Mut<'_, ParamBook>| {
                let none = Names::default();
                let appliers = world
                    .get_resource::<ModifierBook>()
                    .map_or(&none, ModifierBook::appliers);
                params.resolve(appliers);
            });
        }

        /// The place of each of `stats` in the match's book, which a world that names no stat
        /// need not have.
        pub fn stat_places<'s>(
            world: &World,
            stats: impl IntoIterator<Item = &'s Stat>,
        ) -> BTreeMap<&'s Stat, StatId> {
            stats
                .into_iter()
                .map(|stat| {
                    let book = world.resource::<StatBook>();
                    (stat, book.named(stat).expect("a test names its stats"))
                })
                .collect()
        }
    }
}
