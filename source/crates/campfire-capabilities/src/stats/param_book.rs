use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{Capability, TickRate};

use crate::declare::book::Book;
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

    /// The param at `place` a modifier's number reads: `modifier`'s own, or that of `action`,
    /// which applied it, at `rank`, of `source`, as its way checked it holds. A scaling table's
    /// is live, read again as its source changes.
    pub(crate) fn modifier_param(
        &self,
        modifier: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
        place: &ParamPlace,
        source: Option<&ParamSource<'_>>,
    ) -> ParamRead {
        const CHECKED: &str = "its way checked the param";
        let (owner, at) = match place {
            ParamPlace::Own(at) => (ParamOwner::Modifier(modifier), usize::from(*at)),
            ParamPlace::Applier(name) => {
                let action = action.expect(CHECKED);
                let at = self.actions().named(action.index(), name.as_str());
                (ParamOwner::Action(action), at.expect(CHECKED))
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

    /// Whether `action` at `rank`, or no ability, gives the param `name` a modifier reads where
    /// it declares none: a value at `rank` that is a number, or with a `rate` a time in ticks at
    /// it, which no scaling table gives, as its source's stats are known only as the modifier
    /// applies.
    pub(crate) fn gives(
        &self,
        action: Option<ActionId>,
        rank: Rank,
        name: &DeclaredName,
        rate: Option<TickRate>,
    ) -> Result<(), ParamProblem> {
        let action = action.ok_or(ParamProblem::Missing)?;
        let (table, run) = (self.actions(), action.index());
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

    /// Whether it holds the params of `action`.
    pub(crate) fn has_action(&self, action: ActionId) -> bool {
        self.actions().has_run(action.index())
    }

    /// Whether `live` names a scaling param it holds, with a value at `rank`.
    pub(crate) fn holds_live(&self, live: LiveParam, rank: Rank) -> bool {
        let TableRun { table, run } = self.table(live.owner);
        let at = usize::from(live.at);
        at < table.len(run) && table.scales(run, at) && table.has_rank(run, at, rank)
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
            ParamOwner::Action(action) => TableRun {
                table: self.actions(),
                run: action.index(),
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

impl Book for ParamBook {
    const CAPABILITY: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
pub(crate) mod internals {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::stats::param_book::ParamBook;
    use crate::stats::stat_id::StatId;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    impl ParamBook {
        /// Adds the params of the next action, each stat at its id `stat` gives; its run.
        pub(crate) fn push_action(
            &mut self,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) -> usize {
            Arc::make_mut(&mut self.0).push_action(params, stat)
        }

        /// Adds the params of the next modifier, each stat at its id `stat` gives; its run.
        pub(crate) fn push_modifier(
            &mut self,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) -> usize {
            Arc::make_mut(&mut self.0).push_modifier(params, stat)
        }
    }
}
