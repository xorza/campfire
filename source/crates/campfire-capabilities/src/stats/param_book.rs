use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::TickRate;

use crate::scripts::error::ParamProblem;
use crate::stats::live_param::{LiveParam, ParamOwner};
use crate::stats::modifier_data::ModifierData;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::param_read::ParamRead;
use crate::stats::param_source::ParamSource;
use crate::stats::param_table::ParamTable;
use crate::stats::stat_id::StatId;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::values::declared_name::DeclaredName;
use crate::values::param::Param;
use crate::values::rank::Rank;
use crate::values::stat::Stat;

/// The params of every action and every modifier of a match, one run each, by id: package data
/// that the stat engine and the script frame both read, shared, never copied.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ParamBook(Arc<ParamTables>);

/// The two tables of a `ParamBook`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParamTables {
    pub(crate) actions: ParamTable,
    pub(crate) modifiers: ParamTable,
}

/// A table of params, and the run of one owner's params there.
#[derive(Debug, Clone, Copy)]
struct TableRun<'a> {
    table: &'a ParamTable,
    run: usize,
}

impl ParamBook {
    /// The book of `tables`, which the load built.
    pub(crate) fn new(tables: ParamTables) -> ParamBook {
        ParamBook(Arc::new(tables))
    }

    /// Every action's params, one run per action.
    pub(crate) fn actions(&self) -> &ParamTable {
        &self.0.actions
    }

    /// Every modifier's params, one run per modifier.
    pub(crate) fn modifiers(&self) -> &ParamTable {
        &self.0.modifiers
    }

    /// The param at `place` a modifier's number reads: `modifier`'s own, or that of `ability`,
    /// which applied it, at `rank`, of `source`, as its way checked it holds. A scaling table's
    /// is live, read again as its source changes.
    pub(crate) fn modifier_param(
        &self,
        modifier: ModifierId,
        ability: Option<ActionId>,
        rank: Rank,
        place: &ParamPlace,
        source: Option<&ParamSource<'_>>,
    ) -> ParamRead {
        const CHECKED: &str = "its way checked the param";
        let (owner, at) = match place {
            ParamPlace::Own(at) => (ParamOwner::Modifier(modifier), usize::from(*at)),
            ParamPlace::Applier(name) => {
                let ability = ability.expect(CHECKED);
                let at = self.actions().named(ability.index(), name.as_str());
                (ParamOwner::Action(ability), at.expect(CHECKED))
            }
        };
        let TableRun { table, run } = self.table(owner);
        let value = table.value(run, at, rank, source).to_num().expect(CHECKED);
        let live = table.scales(run, at).then(|| LiveParam {
            owner,
            at: u16::try_from(at).expect("params fit u16"),
        });
        ParamRead { value, live }
    }

    /// Whether `ability` at `rank`, or no ability, gives the param `name` a modifier reads where
    /// it declares none: a value at `rank` that is a number, or with a `rate` a time in ticks at
    /// it, which no scaling table gives, as its source's stats are known only as the modifier
    /// applies.
    pub(crate) fn gives(
        &self,
        ability: Option<ActionId>,
        rank: Rank,
        name: &DeclaredName,
        rate: Option<TickRate>,
    ) -> Result<(), ParamProblem> {
        let ability = ability.ok_or(ParamProblem::Missing)?;
        let (table, run) = (self.actions(), ability.index());
        let at = table
            .named(run, name.as_str())
            .ok_or(ParamProblem::Missing)?;
        if !table.has_rank(run, at, rank) {
            return Err(ParamProblem::Short);
        }
        if table.scales(run, at) {
            return match rate {
                Some(_) => Err(ParamProblem::ScalingTime),
                None => Ok(()),
            };
        }
        let value = table.value(run, at, rank, None).to_num();
        let value = value.ok_or(ParamProblem::Overflow)?;
        match rate {
            Some(rate) if ModifierData::ticks(value, rate).is_none() => Err(ParamProblem::Time),
            _ => Ok(()),
        }
    }

    /// Whether it holds the params of `ability`.
    pub(crate) fn has_action(&self, ability: ActionId) -> bool {
        self.actions().has_run(ability.index())
    }

    /// Whether `live` names a scaling param it holds, with a value at `rank`.
    pub(crate) fn holds_live(&self, live: LiveParam, rank: Rank) -> bool {
        let TableRun { table, run } = self.table(live.owner);
        let at = usize::from(live.at);
        table.has_run(run)
            && at < table.len(run)
            && table.scales(run, at)
            && table.has_rank(run, at, rank)
    }

    /// The value of live param `live` at `rank` of `source`.
    pub(crate) fn live_value(
        &self,
        live: LiveParam,
        rank: Rank,
        source: Option<&ParamSource<'_>>,
    ) -> Num {
        let TableRun { table, run } = self.table(live.owner);
        let value = table.value(run, usize::from(live.at), rank, source);
        value.to_num().expect("its way checked the param")
    }

    /// The table of `owner`'s params, and its run there.
    fn table(&self, owner: ParamOwner) -> TableRun<'_> {
        match owner {
            ParamOwner::Modifier(modifier) => TableRun {
                table: self.modifiers(),
                run: modifier.index(),
            },
            ParamOwner::Action(ability) => TableRun {
                table: self.actions(),
                run: ability.index(),
            },
        }
    }
}

impl ParamTables {
    /// Adds the params of the next action, each stat at its id `stat` gives; its run.
    pub(crate) fn push_action(
        &mut self,
        params: &BTreeMap<DeclaredName, Param>,
        stat: impl Fn(&Stat) -> StatId,
    ) -> usize {
        self.actions.push(params, stat)
    }

    /// Adds the params of the next modifier, each stat at its id `stat` gives; its run.
    pub(crate) fn push_modifier(
        &mut self,
        params: &BTreeMap<DeclaredName, Param>,
        stat: impl Fn(&Stat) -> StatId,
    ) -> usize {
        self.modifiers.push(params, stat)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::stats::stats_call::StatsCall;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bevy_ecs::world::World;

    use crate::scripts::ctx::Ctx;
    use crate::stats::param_book::ParamBook;
    use crate::stats::stat_id::StatId;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    impl ParamBook {
        /// Adds the params of `action`, the one `world`'s action book loaded last, each stat at
        /// its place `stat` gives.
        pub(crate) fn load_action(
            world: &mut World,
            action: ActionId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let mut book = world.resource_mut::<ParamBook>();
            let run = Arc::make_mut(&mut book.0).push_action(params, stat);
            assert_eq!(run, action.index(), "one run of params per action");
            ParamBook::share(world);
        }

        /// Adds the params of `modifier`, the one `world`'s modifier book loaded last, each stat
        /// at its place `stat` gives.
        pub(crate) fn load_modifier(
            world: &mut World,
            modifier: ModifierId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let mut book = world.resource_mut::<ParamBook>();
            let run = Arc::make_mut(&mut book.0).push_modifier(params, stat);
            assert_eq!(run, modifier.index(), "one run of params per modifier");
            ParamBook::share(world);
        }

        /// Gives `world`'s script frame, when it has one, the book as it is now.
        fn share(world: &World) {
            if let Some(ctx) = world.get_non_send::<Ctx>() {
                let params = world.resource::<ParamBook>().clone();
                StatsCall::share_params(&mut ctx.frame(), params);
            }
        }
    }
}
