use std::ops::Range;
use std::sync::Arc;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::{
    AnyKeptRows, ApiError, Checked, ColumnRows, KeptRows, RowFill, RunMove, StateValue, View,
    ViewColumn,
};
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank};

use crate::level::Level;
use crate::life::Life;
use crate::meter::Meter;
use crate::modifier_book::ModifierBook;
use crate::modifier_handle::{HandleOf, ModifierHandle};
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;
use crate::pool_book::PoolBook;
use crate::pool_id::PoolId;
use crate::pools::Pools;
use crate::stat_id::StatId;
use crate::stats_call::StatsCall;
use crate::unit_pools::PoolMeters;
use crate::unit_stats::UnitStats;

/// The parts of a unit the stats read into its row.
pub type RowReads = (
    Option<&'static Level>,
    Option<&'static Life>,
    Option<&'static Pools>,
    Option<&'static UnitStats>,
    Option<&'static Modifiers>,
    Option<&'static ModifierClocks>,
);

/// What stats adds to the script view: the stats, pools and modifiers the mode declares, and each
/// unit's level, pools, values of its stats and the modifiers it carries with their script
/// state, a row each.
#[derive(Debug, Default)]
pub struct StatsColumn {
    /// The mode's life pool, as the read found it.
    life_pool: Option<PoolId>,
    modifier_book: ModifierBook,
    rows: KeptRows<StatsRows>,
}

/// The rows of one read of the stats column.
#[derive(Debug, Default, PartialEq)]
struct StatsRows {
    rows: Vec<StatsRow>,
    stats: Vec<Num>,
    modifiers: Vec<ModifierRow>,
    modifier_state: Vec<StateValue>,
}

/// A unit's stats as the view read them: its level, life and other pools, none for a unit with no
/// stats, and its runs of stat values and of modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StatsRow {
    level: Option<u32>,
    life: Option<Life>,
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
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        self.life_pool = world.resource::<PoolBook>().life();
        let book = world.resource::<ModifierBook>();
        if !book.same(&self.modifier_book) {
            self.modifier_book = book.clone();
        }
        false
    }
}

impl ColumnRows for StatsRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.stats.clear();
        self.modifiers.clear();
        self.modifier_state.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let (first, last) = (&from.rows[rows.start], &from.rows[rows.end - 1]);
        let values = RunMove::new(first.stats.start, self.stats.len());
        let carried = RunMove::new(first.modifiers.start, self.modifiers.len());
        self.rows.extend(from.rows[rows].iter().map(|row| StatsRow {
            level: row.level,
            life: row.life,
            pools: row.pools,
            stats: values.of(&row.stats),
            modifiers: carried.of(&row.modifiers),
        }));
        let stats = first.stats.start as usize..last.stats.end as usize;
        self.stats.extend_from_slice(&from.stats[stats]);
        let modifiers =
            &from.modifiers[first.modifiers.start as usize..last.modifiers.end as usize];
        if let (Some(head), Some(tail)) = (modifiers.first(), modifiers.last()) {
            let script = RunMove::new(head.state.start, self.modifier_state.len());
            self.modifiers
                .extend(modifiers.iter().map(|modifier| ModifierRow {
                    state: script.of(&modifier.state),
                    ..modifier.clone()
                }));
            let run = head.state.start as usize..tail.state.end as usize;
            self.modifier_state
                .extend_from_slice(&from.modifier_state[run]);
        }
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

impl StatsRows {
    /// Adds the row of a unit at `level`, with `life` and `pools`, the values `stats` of the stats
    /// it carries, and `modifiers` with their `clocks`.
    fn push(
        &mut self,
        level: Option<u32>,
        (life, pools): (Option<&Life>, Option<&Pools>),
        stats: &[Num],
        modifiers: Option<&Modifiers>,
        clocks: Option<&ModifierClocks>,
    ) {
        let stats_start = len(self.stats.len());
        self.stats.extend_from_slice(stats);
        let modifiers_start = len(self.modifiers.len());
        let carried = modifiers.zip(clocks).into_iter();
        let carried = carried.flat_map(|(modifiers, clocks)| {
            let held = modifiers.iter().enumerate();
            held.map(move |(at, instance)| (instance, clocks.state(at)))
        });
        for (instance, state) in carried {
            let start = len(self.modifier_state.len());
            self.modifier_state.extend_from_slice(state);
            self.modifiers.push(ModifierRow {
                id: instance.id,
                source: instance.source,
                stacks: instance.stacks,
                state: start..len(self.modifier_state.len()),
            });
        }
        self.rows.push(StatsRow {
            level,
            life: life.copied(),
            pools: pools.copied(),
            stats: stats_start..len(self.stats.len()),
            modifiers: modifiers_start..len(self.modifiers.len()),
        });
    }

    /// The values of the stats of the unit in row `row`.
    fn stats(&self, row: usize) -> &[Num] {
        let run = &self.rows[row].stats;
        &self.stats[run.start as usize..run.end as usize]
    }

    /// The modifiers the unit in row `row` carries.
    fn run(&self, row: usize) -> &[ModifierRow] {
        let run = &self.rows[row].modifiers;
        &self.modifiers[run.start as usize..run.end as usize]
    }

    /// The script state of `held`.
    fn state(&self, held: &ModifierRow) -> &[StateValue] {
        &self.modifier_state[held.state.start as usize..held.state.end as usize]
    }
}

impl StatsColumn {
    /// Adds the row of a unit at `level`, with `life` and `pools`, the values `stats` of the stats
    /// it carries, and `modifiers` with their `clocks`.
    pub fn push(
        &mut self,
        level: Option<u32>,
        pools: (Option<&Life>, Option<&Pools>),
        stats: &[Num],
        modifiers: Option<&Modifiers>,
        clocks: Option<&ModifierClocks>,
    ) {
        self.rows
            .now_mut()
            .push(level, pools, stats, modifiers, clocks);
    }

    /// What `read` gives of the column; an error for a match with no stats.
    fn read<R>(view: &View, read: impl FnOnce(&StatsColumn) -> Checked<R>) -> Checked<R> {
        view.column(read)
            .unwrap_or_else(|| Err(ApiError::NoStats.fail().into()))
    }

    /// The level of the unit in row `row`; an error for a unit with no stats.
    pub(crate) fn level(view: &View, row: usize) -> Checked<u32> {
        StatsColumn::read(view, |column| {
            Ok(column.rows.now().rows[row]
                .level
                .ok_or_else(|| ApiError::NoStats.fail())?)
        })
    }

    /// The value of `stat` of the unit in row `row`; an error for a unit with no stats.
    pub(crate) fn stat(view: &View, row: usize, stat: StatId) -> Checked<Num> {
        StatsColumn::read(view, |column| {
            column
                .rows
                .now()
                .stats(row)
                .get(stat.index())
                .copied()
                .ok_or_else(|| ApiError::NoStats.fail().into())
        })
    }

    /// What `read` gives of `pool` of the unit in row `row`; an error for a pool the unit does
    /// not have.
    pub(crate) fn pool(
        view: &View,
        row: usize,
        pool: PoolId,
        read: fn(Meter) -> Num,
    ) -> Checked<Num> {
        StatsColumn::read(view, |column| {
            let row = &column.rows.now().rows[row];
            let pools = PoolMeters::new(column.life_pool, row.life.as_ref(), row.pools.as_ref());
            pools
                .meter(pool)
                .map(read)
                .ok_or_else(|| ApiError::NoPool.fail().into())
        })
    }

    /// Whether `action` at `rank`, or no ability at rank 1, applies modifier `id` with every
    /// param it reads, of `call`'s params; an error with the param's problem when not.
    pub(crate) fn check_way(
        view: &View,
        call: &StatsCall,
        id: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
    ) -> Checked<()> {
        let rate = view.rate();
        StatsColumn::read(view, |column| {
            let checked = column
                .modifier_book
                .check_way(id, action, rank, call.params(), rate);
            Ok(checked.map_err(|problem| ApiError::ModifierParam(problem).fail())?)
        })
    }

    /// Whether the unit in row `row` carries the modifier `id`.
    pub(crate) fn has_modifier(view: &View, row: usize, id: ModifierId) -> Checked<bool> {
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
        call: &mut StatsCall,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) -> ModifierHandle {
        if let Some(handle) = call.handed(carrier, id, source) {
            view.column(|column: &StatsColumn| column.apply_again(handle, id))
                .expect("a match that applies a modifier has stats");
            return handle.clone();
        }
        let row = view.row_index(carrier);
        let spare = call.spare();
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
                        column.rows.now().state(held),
                    ),
                    None => (1, spec.state.initial()),
                };
                let of = HandleOf {
                    carrier,
                    id,
                    source,
                    stacks,
                };
                let schema = Arc::clone(&spec.state);
                ModifierHandle::new(spare, of, state, schema, view.clone())
            })
            .expect("a match that applies a modifier has stats");
        call.hand_out(handle.clone());
        handle
    }

    /// The handle of the instance `of`, as a call sees it: its stacks, and `state`, filling
    /// `spare` again when given.
    pub fn held_handle(
        view: &View,
        spare: Option<ModifierHandle>,
        of: HandleOf,
        state: &[StateValue],
    ) -> ModifierHandle {
        let schema = view
            .column(|column: &StatsColumn| Arc::clone(&column.modifier_book.get(of.id).spec.state))
            .expect("a match with modifiers has stats");
        ModifierHandle::new(spare, of, state, schema, view.clone())
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
            data.state.clone_from_slice(spec.state.initial());
        } else {
            data.stacks = spec.reapply.stacks(data.stacks, spec.max_stacks);
        }
    }

    /// The modifiers the unit in row `row` carries.
    fn run(&self, row: usize) -> &[ModifierRow] {
        self.rows.now().run(row)
    }

    /// Adds the unit's level, pools, stats and modifiers to the stats' column of the script view.
    pub(super) fn fill_row(
        parts: ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, StatsColumn>,
    ) {
        let (level, life, pools, stats, modifiers, clocks) = parts;
        let stats = stats.map_or(&[][..], UnitStats::values);
        fill.column.push(
            level.map(|level| level.get()),
            (life, pools),
            stats,
            modifiers,
            clocks,
        );
    }
}

/// A length of one of the column's buffers, as its runs count it.
fn len(len: usize) -> u32 {
    u32::try_from(len).expect("a view's runs fit u32")
}
