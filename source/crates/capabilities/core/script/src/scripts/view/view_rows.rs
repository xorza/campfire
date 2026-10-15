use std::mem;

use campfire_common::Tick;
use campfire_geometry::{Bounds, Metric};
use campfire_sim::StableId;
use campfire_values::Relations;

use crate::scripts::view::unit_row::UnitRow;
use crate::scripts::view::view_column::{ViewColumn, ViewColumns};

/// The units scripts see, those with a position and a team, as the running phase of the tick
/// began, and what every row reads besides them: the rows a fill writes before each phase that
/// runs scripts.
#[derive(Debug)]
pub struct ViewRows {
    /// The tick the units were read in.
    now: Tick,
    /// By stable id.
    units: Vec<UnitRow>,
    /// How the teams regard each other, as the units were read.
    relations: Relations,
    metric: Metric,
    bounds: Bounds,
    /// What each capability above the core reads of the units, and its getters read besides.
    columns: ViewColumns,
    /// Whether the next read must fill every row, as what the rows derive from besides the
    /// units' parts changed.
    refill: bool,
}

/// What a read takes besides the units: the tick, the relations, and the map's metric and bounds.
#[derive(Debug, Clone, Copy)]
pub struct ReadOf<'a> {
    pub now: Tick,
    pub relations: &'a Relations,
    pub metric: Metric,
    pub bounds: Bounds,
}

impl ViewRows {
    /// Starts a read of what `of` gives. Whether every row must be filled: when asked since the
    /// last read, or as the relations changed.
    pub fn begin(&mut self, of: ReadOf<'_>) -> bool {
        let refill = mem::take(&mut self.refill) || *of.relations != self.relations;
        self.now = of.now;
        self.relations.clone_from(of.relations);
        self.metric = of.metric;
        self.bounds = of.bounds;
        refill
    }

    /// The rows, by stable id.
    pub fn units(&self) -> &[UnitRow] {
        &self.units
    }

    /// The rows, for a read to fill.
    pub const fn units_mut(&mut self) -> &mut Vec<UnitRow> {
        &mut self.units
    }

    /// The place of unit `id` among the rows, when the read found it.
    pub fn index(&self, id: StableId) -> Option<usize> {
        self.units.binary_search_by_key(&id, |row| row.id).ok()
    }

    /// The row of unit `id`, when the read found it.
    pub fn row(&self, id: StableId) -> Option<&UnitRow> {
        Some(&self.units[self.index(id)?])
    }

    /// The tick the units were read in.
    pub const fn now(&self) -> Tick {
        self.now
    }

    /// How the teams regard each other, as the units were read.
    pub const fn relations(&self) -> &Relations {
        &self.relations
    }

    pub const fn metric(&self) -> Metric {
        self.metric
    }

    /// The map's bounds, as the units were read.
    pub const fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub const fn columns(&self) -> &ViewColumns {
        &self.columns
    }

    /// The columns, for a read to fill.
    pub const fn columns_mut(&mut self) -> &mut ViewColumns {
        &mut self.columns
    }

    /// Adds `column`, which a source fills.
    pub fn add_column<C: ViewColumn>(&mut self, column: C) {
        self.columns.add(column);
    }

    /// The column of type `C`, to change, when one was added.
    pub(crate) fn column_mut<C: ViewColumn>(&mut self) -> Option<&mut C> {
        self.columns.get_mut()
    }

    /// Makes the next read fill every row.
    pub const fn refill_next(&mut self) {
        self.refill = true;
    }
}

impl Default for ViewRows {
    fn default() -> ViewRows {
        ViewRows {
            now: Tick::ZERO,
            units: Vec::new(),
            relations: Relations::default(),
            metric: Metric::default(),
            bounds: Bounds::WORLD,
            columns: ViewColumns::default(),
            refill: true,
        }
    }
}
