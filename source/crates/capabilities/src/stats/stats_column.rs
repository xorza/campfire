use std::ops::Range;
use std::sync::Arc;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_value::StateValue;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifiers::Modifiers;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::units::modifier_id::ModifierId;
use crate::units::script_view::View;
use crate::units::view_column::ViewColumn;
use crate::values::declared_name::DeclaredName;
use crate::values::stat::Stat;

/// What stats adds to the script view: the stats, pools and modifiers the mode declares, and each
/// unit's level, pools, values of its stats and the modifiers it carries with their script
/// state, a row each.
#[derive(Debug, Default)]
pub(crate) struct StatsColumn {
    /// The stats the mode declares, in the order units' runs of stats hold them.
    stat_names: Arc<[Stat]>,
    /// The pools the mode declares, by pool id.
    pool_names: Arc<[DeclaredName]>,
    modifier_book: ModifierBook,
    rows: Vec<StatsRow>,
    stats: Vec<Num>,
    modifiers: Vec<ModifierRow>,
    modifier_state: Vec<StateValue>,
}

/// A unit's stats as the view read them: its level and pools, none for a unit with no stats, and
/// its runs of stat values and of modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StatsRow {
    level: Option<u32>,
    pools: Option<Pools>,
    stats: Range<u32>,
    modifiers: Range<u32>,
}

/// A modifier a unit carries, as the view read it: which, from whom, its stacks, and its run of
/// script state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModifierRow {
    id: ModifierId,
    source: Option<StableId>,
    stacks: u32,
    state: Range<u32>,
}

impl ViewColumn for StatsColumn {
    fn clear(&mut self) {
        self.rows.clear();
        self.stats.clear();
        self.modifiers.clear();
        self.modifier_state.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl StatsColumn {
    /// Adds the row of a unit at `level`, with `pools`, the values `stats` of the stats it
    /// carries, and `modifiers`.
    pub(crate) fn push(
        &mut self,
        level: Option<u32>,
        pools: Option<Pools>,
        stats: &[Num],
        modifiers: Option<&Modifiers>,
    ) {
        let stats_start = len(self.stats.len());
        self.stats.extend_from_slice(stats);
        let modifiers_start = len(self.modifiers.len());
        for instance in modifiers.into_iter().flat_map(Modifiers::iter) {
            let start = len(self.modifier_state.len());
            self.modifier_state.extend_from_slice(&instance.state);
            self.modifiers.push(ModifierRow {
                id: instance.id,
                source: instance.source,
                stacks: instance.stacks,
                state: start..len(self.modifier_state.len()),
            });
        }
        self.rows.push(StatsRow {
            level,
            pools,
            stats: stats_start..len(self.stats.len()),
            modifiers: modifiers_start..len(self.modifiers.len()),
        });
    }

    /// Names the stats the mode declares to scripts, in the order units' runs hold them.
    pub(crate) fn share_stat_names(view: &View, names: Arc<[Stat]>) {
        view.column_mut(|column: &mut StatsColumn| column.stat_names = names);
    }

    /// Names the pools the mode declares to scripts, by pool id.
    pub(crate) fn share_pool_names(view: &View, names: Arc<[DeclaredName]>) {
        view.column_mut(|column: &mut StatsColumn| column.pool_names = names);
    }

    /// Shares the match's modifiers, as the load built them.
    pub(crate) fn share_modifiers(view: &View, book: ModifierBook) {
        view.column_mut(|column: &mut StatsColumn| column.modifier_book = book);
    }

    /// What `read` gives of the column; an error for a match with no stats.
    fn read<R>(view: &View, read: impl FnOnce(&StatsColumn) -> Checked<R>) -> Checked<R> {
        view.column(read)
            .unwrap_or_else(|| Err(ApiError::NoStats.fail().into()))
    }

    /// The level of the unit in row `row`; an error for a unit with no stats.
    pub(crate) fn level(view: &View, row: usize) -> Checked<u32> {
        StatsColumn::read(view, |column| {
            Ok(column.rows[row]
                .level
                .ok_or_else(|| ApiError::NoStats.fail())?)
        })
    }

    /// The value of stat `name` of the unit in row `row`; an error for a stat the mode does not
    /// declare, or a unit with no stats.
    pub(crate) fn stat_named(view: &View, row: usize, name: &str) -> Checked<Num> {
        StatsColumn::read(view, |column| {
            let at = column
                .stat_names
                .binary_search_by(|stat| stat.order_to(name))
                .ok()
                .ok_or_else(|| ApiError::UnknownStat.fail())?;
            let run = &column.rows[row].stats;
            column.stats[run.start as usize..run.end as usize]
                .get(at)
                .copied()
                .ok_or_else(|| ApiError::NoStats.fail().into())
        })
    }

    /// What `read` gives of pool `name` of the unit in row `row`; an error for a pool the mode
    /// does not declare, or one the unit does not have.
    pub(crate) fn pool(
        view: &View,
        row: usize,
        name: &str,
        read: fn(&Pools, PoolId) -> Option<Num>,
    ) -> Checked<Num> {
        let pool = StatsColumn::pool_named(view, name)?;
        StatsColumn::read(view, |column| {
            column.rows[row]
                .pools
                .and_then(|pools| read(&pools, pool))
                .ok_or_else(|| ApiError::NoPool.fail().into())
        })
    }

    /// The pool `name`; an error for one the mode does not declare.
    pub(crate) fn pool_named(view: &View, name: &str) -> Checked<PoolId> {
        StatsColumn::pool_id_named(view, name).ok_or_else(|| ApiError::UnknownPool.fail().into())
    }

    /// The pool `name`; `None` for one the mode does not declare.
    pub(crate) fn pool_id_named(view: &View, name: &str) -> Option<PoolId> {
        view.column(|column: &StatsColumn| {
            let at = column
                .pool_names
                .iter()
                .position(|pool| pool.as_str() == name)?;
            let pool = u8::try_from(at).ok().and_then(PoolId::new);
            Some(pool.expect("the load keeps the pools within the limit"))
        })
        .flatten()
    }

    /// The modifier `name` of `package`; an error when it declares none.
    pub(crate) fn modifier_named(view: &View, package: u16, name: &str) -> Checked<ModifierId> {
        StatsColumn::read(view, |column| {
            let found = column.modifier_book.named(package, name);
            Ok(found.ok_or_else(|| ApiError::UnknownModifier.fail())?)
        })
    }

    /// Whether the unit in row `row` carries the modifier `name` of `package`.
    pub(crate) fn has_modifier(view: &View, row: usize, package: u16, name: &str) -> Checked<bool> {
        let id = StatsColumn::modifier_named(view, package, name)?;
        StatsColumn::read(view, |column| {
            Ok(column.run(row).iter().any(|modifier| modifier.id == id))
        })
    }

    /// The handle of the instance of `id` from `source` on `carrier` that an application in the
    /// running call adds or applies again, as the call sees it: a new one's one stack and first
    /// state, or a held one's, a stack more when it stacks, up to its limit. An instance the call
    /// took a handle to before, in `handles`, keeps that handle, so the call sees one instance
    /// once; one it removed is new again.
    pub(crate) fn applied_handle(
        view: &View,
        handles: &mut Vec<ModifierHandle>,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) -> ModifierHandle {
        if let Some(handle) = handles.iter().find(|handle| handle.is(carrier, id, source)) {
            view.column(|column: &StatsColumn| column.apply_again(handle, id))
                .expect("a match that applies a modifier has stats");
            return handle.clone();
        }
        let row = view.row_index(carrier);
        let handle = view
            .column(|column: &StatsColumn| {
                let spec = &column.modifier_book.get(id).spec;
                let held = row.and_then(|row| {
                    column
                        .run(row)
                        .iter()
                        .find(|modifier| modifier.id == id && modifier.source == source)
                });
                let (stacks, state) = match held {
                    Some(held) => (
                        spec.reapply.stacks(held.stacks, spec.max_stacks),
                        column.modifier_state[held.state.start as usize..held.state.end as usize]
                            .to_vec(),
                    ),
                    None => (1, spec.initial.to_vec()),
                };
                let fields = Arc::clone(&spec.fields);
                ModifierHandle::new(carrier, id, source, stacks, state, fields, view.clone())
            })
            .expect("a match that applies a modifier has stats");
        handles.push(handle.clone());
        handle
    }

    /// The handle of `carrier`'s instance of `id` from `source`, as a call sees it: `stacks`
    /// and `state`.
    pub(crate) fn held_handle(
        view: &View,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        state: Vec<StateValue>,
    ) -> ModifierHandle {
        let fields = view
            .column(|column: &StatsColumn| Arc::clone(&column.modifier_book.get(id).spec.fields))
            .expect("a match with modifiers has stats");
        ModifierHandle::new(carrier, id, source, stacks, state, fields, view.clone())
    }

    /// Applies `id` again through `handle`, which the running call took: a removed instance is
    /// new again, and a held one gains a stack when it stacks, up to its limit.
    fn apply_again(&self, handle: &ModifierHandle, id: ModifierId) {
        let spec = &self.modifier_book.get(id).spec;
        let mut data = handle.data();
        if data.removed {
            data.removed = false;
            data.written = false;
            data.stacks = 1;
            data.state.clone_from_slice(&spec.initial);
        } else {
            data.stacks = spec.reapply.stacks(data.stacks, spec.max_stacks);
        }
    }

    /// The modifiers the unit in row `row` carries.
    fn run(&self, row: usize) -> &[ModifierRow] {
        let run = &self.rows[row].modifiers;
        &self.modifiers[run.start as usize..run.end as usize]
    }
}

/// A length of one of the column's buffers, as its runs count it.
fn len(len: usize) -> u32 {
    u32::try_from(len).expect("a view's runs fit u32")
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::stats::stat_id::StatId;
    use crate::stats::stats_column::StatsColumn;
    use crate::values::stat::Stat;

    impl StatsColumn {
        /// The place of `stat` among the stats the mode declares; `None` when it does not
        /// declare it.
        pub(crate) fn stat_index(&self, stat: &Stat) -> Option<StatId> {
            let at = self.stat_names.binary_search(stat).ok()?;
            Some(StatId::new(at))
        }
    }
}
