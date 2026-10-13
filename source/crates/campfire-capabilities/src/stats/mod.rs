//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_sim::{Capability, SimEdge, SimPlace, SimSet, SimTick, SimWorld, StableId, TickRate};

use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::module_set::ModuleSet;
use crate::declare::set_order::SetOrder;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::grant_pass::GrantPass;
use crate::stats::grants::{Grant, GrantChange, Grants};
use crate::stats::held_modifiers::HeldModifiers;
use crate::stats::held_pass::HeldPass;
use crate::stats::level::Level;
use crate::stats::live_carriers::LiveCarriers;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_ends::ModifierEnds;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pool_book::PoolBook;
use crate::stats::pools::Pools;
use crate::stats::refresh::Refresh;
use crate::stats::stat_book::StatBook;
use crate::stats::stats_api::StatsApi;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::{RowReads, StatsColumn};
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::body_unit::BodyUnit;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::modifier_id::ModifierId;
use crate::units::player::Player;
use crate::units::tag::Tag;
use crate::values::rank::Rank;

pub(crate) mod application;
pub(crate) mod applier;
pub(crate) mod carried_mut;
pub(crate) mod error;
pub(crate) mod grant_pass;
pub(crate) mod grants;
pub(crate) mod held_modifiers;
pub(crate) mod held_pass;
pub(crate) mod instance;
pub(crate) mod level;
pub(crate) mod life;
pub(crate) mod lifetime;
pub(crate) mod live_carriers;
pub(crate) mod live_param;
pub(crate) mod meter;
pub(crate) mod modifier_book;
pub(crate) mod modifier_clocks;
pub(crate) mod modifier_data;
pub(crate) mod modifier_ends;
pub(crate) mod modifier_handle;
pub(crate) mod modifier_params;
pub(crate) mod modifier_spec;
pub(crate) mod modifier_state_field;
pub(crate) mod modifiers;
pub(crate) mod param_book;
pub(crate) mod param_read;
pub(crate) mod param_source;
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
pub(crate) mod stat_books;
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
pub(crate) mod stats_effect;
pub(crate) mod stats_world;
pub(crate) mod unit_pools;
pub(crate) mod unit_stats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

/// The steps of `stats`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StatsSet {
    /// The modifiers and stacks that hold no longer end.
    Expire,
    /// The living units' pools regenerate.
    Regenerate,
    /// Once the tick's orders and trades are in: the grants the sources pushed apply.
    GrantAtInputs,
    /// Once the attacks going off paid their toggles and before they strike: the grants the
    /// sources pushed apply.
    GrantAtStrike,
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
    const STEPS: [StatsSet; 8] = [
        StatsSet::Expire,
        StatsSet::Regenerate,
        StatsSet::GrantAtInputs,
        StatsSet::GrantAtStrike,
        StatsSet::GrantAtResolve,
        StatsSet::GrantAtEnd,
        StatsSet::ClearDead,
        StatsSet::Hold,
    ];

    /// The steps in the stages, then a refresh in each gap, `SimEdge::Start` first.
    const ALL: [StatsSet; StatsSet::STEPS.len() + 1 + SimSet::ALL.len()] = {
        let steps = StatsSet::STEPS.len();
        let mut all =
            [StatsSet::Refresh(SimEdge::Start); StatsSet::STEPS.len() + 1 + SimSet::ALL.len()];
        let mut at = 0;
        while at < steps {
            all[at] = StatsSet::STEPS[at];
            at += 1;
        }
        let mut stage = 0;
        while stage < SimSet::ALL.len() {
            all[steps + 1 + stage] = StatsSet::Refresh(SimEdge::After(SimSet::ALL[stage]));
            stage += 1;
        }
        all
    };
}

impl ModuleSet for StatsSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Stats);
    const ALL: &'static [StatsSet] = &StatsSet::ALL;

    fn place(self) -> SimPlace {
        match self {
            StatsSet::Expire | StatsSet::Regenerate | StatsSet::GrantAtInputs => {
                SimPlace::Stage(SimSet::Inputs)
            }
            StatsSet::GrantAtStrike => SimPlace::Stage(SimSet::Hit),
            StatsSet::GrantAtResolve | StatsSet::ClearDead | StatsSet::Hold => {
                SimPlace::Stage(SimSet::Resolve)
            }
            StatsSet::GrantAtEnd => SimPlace::Stage(SimSet::Vision),
            StatsSet::Refresh(edge) => SimPlace::Edge(edge),
        }
    }

    fn order(self, o: &mut SetOrder) {
        match self {
            StatsSet::GrantAtInputs => {
                o.after(StatsSet::Expire);
            }
            StatsSet::GrantAtResolve => {
                o.before(StatsSet::ClearDead);
            }
            StatsSet::Hold => {
                o.after(StatsSet::ClearDead);
            }
            StatsSet::Expire
            | StatsSet::Regenerate
            | StatsSet::GrantAtStrike
            | StatsSet::GrantAtEnd
            | StatsSet::ClearDead
            | StatsSet::Refresh(_) => {}
        }
    }
}

impl Module for Stats {
    const CAPABILITY: Option<Capability> = Some(Capability::Stats);
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<StatsSet>();
        d.component::<PlayerModifiers>();
        d.component::<Level>();
        d.component::<Modifiers>();
        d.component::<ModifierClocks>();
        d.component::<Pools>();
        d.scratch::<HeldModifiers>();
        d.scratch::<Grants>();
        d.cache::<LiveCarriers>();
        d.derived::<UnitStats>();
        d.part::<Player, PlayerModifiers>(|_| Some(PlayerModifiers::default()));
        d.book::<ModifierBook>();
        d.book::<StatBook>();
        d.book::<PoolBook>();
        d.column::<StatsColumn, RowReads>(StatsColumn::fill_row);
        d.call_part::<StatsCall>();
        d.api(StatsApi::register);
        d.book::<ByType<Pools>>();
        d.part::<BodyUnit, Level>(|_| Some(Level::default()));
        d.part::<BodyUnit, Modifiers>(|_| Some(Modifiers::default()));
        d.part::<BodyUnit, ModifierClocks>(|_| Some(ModifierClocks::default()));
        d.part::<BodyUnit, Pools>(|at| at.of_type::<Pools>().copied());
    }

    /// Adds stats to a match: in `SimEdge::Start` and in the `SimEdge::After` of each stage,
    /// every unit whose level or modifiers changed, or that is new, has its stats and states
    /// derived again, and the components that hold their effect follow them; as each tick
    /// starts, the living units' pools regenerate. The grants the sources push apply as each
    /// tick starts, before attacks strike, before units die and as the tick ends. With no stat
    /// book, as before a mode loads one, nothing changes.
    fn install(cx: &mut InstallCx<'_>) {
        cx.world.insert_resource(LiveCarriers::default());
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            ModifierEnds::expire_modifiers.in_set(StatsSet::Expire),
            Refresh::regenerate.in_set(StatsSet::Regenerate),
            GrantPass::run.in_set(StatsSet::GrantAtInputs),
            GrantPass::run.in_set(StatsSet::GrantAtStrike),
            GrantPass::run.in_set(StatsSet::GrantAtResolve),
            GrantPass::run.in_set(StatsSet::GrantAtEnd),
            ModifierEnds::clear_dead_modifiers.in_set(StatsSet::ClearDead),
            HeldPass::run.in_set(StatsSet::Hold),
        ));
        for set in StatsSet::ALL {
            if let StatsSet::Refresh(_) = set {
                schedule.add_systems(Refresh::run.in_set(set));
            }
        }
    }
}

impl Stats {
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

    /// Grants `grant` to the unit `unit` from itself at once, as `GrantPass` does in its passes:
    /// a unit type's passive as its unit spawns.
    pub(crate) fn grant(world: &mut World, unit: StableId, grant: Grant) {
        let Some(entity) = world.entity_of(unit) else {
            return;
        };
        let Some(modifiers) = world.get::<Modifiers>(entity) else {
            return;
        };
        match grant.change(modifiers, unit) {
            Some(GrantChange::Apply(rank)) => {
                let applier = Applier {
                    source: Some(unit),
                    action: grant.action,
                    rank,
                    hold: Some(grant.hold),
                };
                Stats::add_modifier(world, unit, grant.modifier, applier, None);
            }
            Some(GrantChange::Release) => {
                if let Some(mut carried) = CarriedMut::of(world, entity) {
                    carried.release(grant.modifier, Some(unit), grant.hold);
                }
            }
            None => {}
        }
    }

    fn add_modifier(
        world: &mut World,
        target: StableId,
        id: ModifierId,
        applier: Applier,
        duration: Option<Ticks>,
    ) {
        let Some(entity) = world.entity_of(target) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let now = world.resource::<SimTick>().start();
        let rate = *world.resource::<TickRate>();
        let book = world.resource::<ModifierBook>();
        let source = applier
            .source
            .and_then(|source| ParamSource::of(world, source));
        let params = world.resource::<ParamBook>();
        let param = |place: &ParamPlace| {
            let (action, rank) = (applier.action, applier.rank);
            params.modifier_param(id, action, rank, place, source.as_ref())
        };
        let application = book.application(id, applier, duration, now, rate, param);
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
    pub(super) fn write_handle(world: &mut World, handle: &ModifierHandle) {
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
    pub(crate) fn modifier_has_way(
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
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_sim::{SimWorld, StableId};

    use crate::stats::Stats;
    use crate::stats::applier::Applier;
    use crate::stats::lifetime::Hold;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::stats_effect::StatsEffect;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::values::rank::Rank;

    impl Stats {
        /// The modifier `name` of `package`, as the match loaded it.
        pub fn modifier(world: &World, package: u16, name: &str) -> Option<ModifierId> {
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
            add.apply_by(world, applier);
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
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::world::{Mut, World};
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{StableId, TickRate};

    use crate::books::Books;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::Stats;
    use crate::stats::life::Life;
    use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::stats::modifier_data::ModifierData;
    use crate::stats::param_book::ParamBook;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_id::StatId;
    use crate::stats::stat_op::StatOp;
    use crate::stats::stat_rule::StatRule;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag_set::TagSet;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    impl Stats {
        /// The life unit `id` has left, a whole amount.
        pub(crate) fn health(world: &World, id: StableId) -> i64 {
            world
                .part::<Life>(id)
                .current()
                .to_int()
                .expect("a whole amount of life")
        }

        /// Adds to the match's books a modifier of no params, script, times or state that changes
        /// each stat of `changes` by its op, by a value its instances hold, and grants `tags`:
        /// what a test's instances are instances of. Its id, after every modifier before it.
        pub(crate) fn push_modifier_changes(
            world: &mut World,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let id = world
                .resource_mut::<ModifierBook>()
                .push_changes(changes, tags);
            Stats::load_modifier_params(world, id, &BTreeMap::new(), |_| StatId::new(0));
            id
        }

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
                // A match that runs no scripts holds no script book, and its modifiers none.
                let none = ScriptBook::default();
                let scripts = world.get_resource::<ScriptBook>().unwrap_or(&none);
                let view = world.non_send::<View>();
                let places = Stats::stat_places(world, data.stats.keys());
                let mut types = view.types_mut();
                let load = ModifierLoad {
                    scripts,
                    types: &mut types,
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
            Books::share::<ModifierBook>(world);
            let places = Stats::stat_places(world, data.params.values().flat_map(Param::stats));
            Stats::load_modifier_params(world, id, &data.params, |stat| places[stat]);
        }

        /// Gives a match with no mode the stat book `book`, with no pool stats; the pools its
        /// scripts name stay named.
        pub(crate) fn load_book(world: &mut World, book: StatBook) {
            Books::load(world, book);
        }

        /// Gives `unit_type` `values` at level 1 in the match's stat book, by the stats' places,
        /// as its `stats` section would.
        pub(crate) fn give_type_stats(world: &mut World, unit_type: UnitType, values: &[Num]) {
            world.resource_mut::<StatBook>().add_base(unit_type, values);
        }

        /// Gives a match with no mode the stat book of `rules`, with no unit type and no pool stats;
        /// the pools its scripts name stay named.
        pub(crate) fn load_rules(world: &mut World, rules: &BTreeMap<Stat, StatRule>) {
            Stats::load_book(world, StatBook::new(rules, Num::MAX));
        }

        /// Adds the params of `action`, the one the match's action book loaded last, each stat at
        /// its place `stat` gives.
        pub(crate) fn load_action_params(
            world: &mut World,
            action: ActionId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let run = world.resource_mut::<ParamBook>().push_action(params, stat);
            assert_eq!(run, action.index(), "one run of params per action");
            Books::share::<ParamBook>(world);
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
            Books::share::<ParamBook>(world);
        }

        /// The place of each of `stats` in the match's book, which a world that names no stat
        /// need not have.
        pub(crate) fn stat_places<'s>(
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

#[cfg(test)]
mod tests;
