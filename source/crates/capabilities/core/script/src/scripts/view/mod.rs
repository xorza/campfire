use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_geometry::{Bounds, Metric, Shape};
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, StableId};
use campfire_values::{Relation, Tag, Team};

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::view::filter::Filter;
use crate::scripts::view::filter_texts::FilterTexts;
use crate::scripts::view::listing::Listing;
use crate::scripts::view::living_unit::LivingUnit;
use crate::scripts::view::match_names::MatchNames;
use crate::scripts::view::target_index::{TargetIndex, TargetQuery};
use crate::scripts::view::unit::Unit;
use crate::scripts::view::unit_row::UnitRow;
use crate::scripts::view::view_column::{ViewColumn, ViewColumns};
use crate::scripts::view::view_names::ViewNames;
use crate::scripts::view::view_rows::ViewRows;

pub(crate) mod filter;
pub(crate) mod filter_texts;
pub(crate) mod filters;
pub(crate) mod kept_rows;
pub(crate) mod listing;
pub(crate) mod living_unit;
pub(crate) mod match_names;
pub(crate) mod row_fill;
pub(crate) mod row_reads;
pub(crate) mod tag_names;
pub(crate) mod target_index;
pub(crate) mod teams;
pub(crate) mod unit;
pub(crate) mod unit_row;
pub(crate) mod view_column;
pub(crate) mod view_names;
pub(crate) mod view_rows;

/// What scripts see, as the host and every handle share it: the match's names, its units, those
/// with a team, as the running phase of the tick began, and the targets' bodies the queries that
/// reach by distance search. The units are read again before each phase that runs scripts, and
/// every call of the phase sees them as they were read. Each part has a cell of its own, so a
/// name lookup does not borrow the rows.
#[derive(Clone)]
pub struct View(Rc<ViewFields>);

/// The fields of the view.
#[derive(Debug)]
struct ViewFields {
    tick_rate: MatchRate,
    names: RefCell<ViewNames>,
    /// The revision of the match's `MatchNames` that `names` derive from; none before the first.
    names_at: Cell<Option<u64>>,
    rows: RefCell<ViewRows>,
    targets: RefCell<TargetIndex>,
    /// The filters scripts gave as text, as the names resolve them.
    filters: RefCell<FilterTexts>,
    /// What reads a world's units into the rows, which the module that holds the units gives.
    read: fn(&mut World),
}

impl View {
    /// The view of a match at `rate`, whose units `read` reads into its rows, with no names
    /// until the load and the mode give them.
    pub fn new(rate: MatchRate, read: fn(&mut World)) -> Self {
        Self(Rc::new(ViewFields {
            tick_rate: rate,
            names: RefCell::default(),
            names_at: Cell::new(None),
            rows: RefCell::default(),
            targets: RefCell::default(),
            filters: RefCell::default(),
            read,
        }))
    }

    /// The names scripts read, as the last read derived them from the match's `MatchNames`.
    pub fn names(&self) -> Ref<'_, ViewNames> {
        self.0.names.borrow()
    }

    /// The units as the running phase read them.
    pub fn rows(&self) -> Ref<'_, ViewRows> {
        self.0.rows.borrow()
    }

    /// The rows, for the read that fills them.
    pub fn rows_mut(&self) -> RefMut<'_, ViewRows> {
        self.0.rows.borrow_mut()
    }

    /// Reads the units of `world` for the phase that begins, and its names, when its `MatchNames`
    /// changed since the read before.
    pub fn read(&self, world: &mut World) {
        self.read_names(world);
        (self.0.read)(world);
        self.0.targets.borrow_mut().forget();
    }

    /// Derives the names from `world`'s `MatchNames`, when they changed since they last did: the
    /// load and the mode change them, a tick does not.
    fn read_names(&self, world: &World) {
        let Some(names) = world.get_resource::<MatchNames>() else {
            return;
        };
        if self.0.names_at.get() != Some(names.revision()) {
            *self.0.names.borrow_mut() = ViewNames::of(names);
            self.0.filters.borrow_mut().clear();
            self.0.names_at.set(Some(names.revision()));
        }
    }

    /// The filter `text` names, with its tags among the match's, resolved once for each text a
    /// script gives again.
    pub fn filter(&self, text: &ScriptText) -> Result<Filter, ApiError> {
        let names = self.names();
        self.0.filters.borrow_mut().filter(text, names.tags())
    }

    /// The map's bounds, as the units were read.
    pub fn bounds(&self) -> Bounds {
        self.rows().bounds()
    }

    pub fn metric(&self) -> Metric {
        self.rows().metric()
    }

    /// The tick the units were read in.
    pub fn now(&self) -> Tick {
        self.rows().now()
    }

    /// The match's tick rate.
    pub fn rate(&self) -> MatchRate {
        self.0.tick_rate
    }

    /// How `of` regards `other`, as the units were read.
    pub fn relation(&self, of: Team, other: Team) -> Relation {
        self.rows().relations().between(of, other)
    }

    /// What `read` gives of the column of type `C`; `None` when none was added.
    pub fn column<C: ViewColumn, R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        self.rows().columns().get().map(read)
    }

    /// Changes the column of type `C` by `write`, when one was added.
    /// The next read fills every row again, as the rows may derive from what `write` changes.
    pub fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        let mut rows = self.0.rows.borrow_mut();
        if let Some(column) = rows.column_mut() {
            write(column);
            rows.refill_next();
        }
    }

    /// Makes the next read fill every row. Bevy clamps each change tick older than it compares
    /// exactly as it checks the world's ticks, but not the last read of the view's queries of
    /// changed parts, so after a check those queries may miss a change.
    pub fn refill_next(&self) {
        self.0.rows.borrow_mut().refill_next();
    }

    /// Changes the rows of the column of type `C` by `write`, as a call's effect changes the
    /// units' parts too, which the next read finds changed.
    pub fn write_rows<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.rows.borrow_mut().column_mut() {
            write(column);
        }
    }

    /// The place among the rows of unit `id`, when the view read it.
    pub fn row_index(&self, id: StableId) -> Option<usize> {
        self.rows().index(id)
    }

    /// Every unit, living or dead, that `keep` keeps, by stable id.
    pub fn units_where(&self, mut keep: impl FnMut(&UnitRow) -> bool) -> ScriptList {
        let rows = self.rows();
        rows.units()
            .iter()
            .enumerate()
            .filter(|(_, row)| keep(row))
            .map(|(at, row)| ScriptValue::of(Unit::new(row.id, at, self.clone())))
            .collect()
    }

    /// Every unit with `tag` that `listing` lists, by stable id.
    pub fn units_tagged(&self, tag: Tag, listing: Listing) -> ScriptList {
        self.units_where(|row| listing.lists(row) && row.tags.tags.contains(tag))
    }

    pub fn row(&self, id: StableId) -> Option<UnitRow> {
        self.rows().row(id).copied()
    }

    /// What `read` reads of the row at `at` among the rows, as a handle names it, in place.
    pub fn read_row<R>(&self, at: usize, read: impl FnOnce(&UnitRow) -> R) -> R {
        read(&self.rows().units()[at])
    }

    /// Calls `visit` with each row the view read, and its place, in order of stable id.
    pub fn each_row(&self, mut visit: impl FnMut(usize, &UnitRow)) {
        for (at, row) in self.rows().units().iter().enumerate() {
            visit(at, row);
        }
    }

    /// The handle of unit `id`, when the view read it.
    pub fn unit(&self, id: StableId) -> Option<Unit> {
        let at = self.row_index(id)?;
        Some(Unit::new(id, at, self.clone()))
    }

    /// The handle of unit `id` as a script value: `()` for none, or a unit the view did not read.
    pub fn unit_value(&self, id: Option<StableId>) -> ScriptValue {
        id.and_then(|id| self.unit(id))
            .map_or(ScriptValue::UNIT, ScriptValue::of)
    }

    /// Unit `id`, when it is a living unit that may be a target.
    pub fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self.row(id).filter(|row| row.targetable)?;
        Some(LivingUnit {
            id,
            position: row.position,
            team: row.team,
            shape: row.shape,
            tags: row.tags.tags,
        })
    }

    /// The living targets whose bodies come within `radius` of `position` in the map's metric, as
    /// an area of that radius reaches, that `filter` selects relative to `of`, and `seen` lets by
    /// the view's columns and their rows, by stable id.
    pub fn find(
        &self,
        of: &Unit,
        position: Vec3,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptList> {
        debug_assert!(radius >= Num::ZERO, "a script's radius is not negative");
        let rows = self.rows();
        let units = rows.units();
        let query = TargetQuery {
            team: units[of.row_index()?].team,
            from: position,
            shape: Shape::POINT,
            radius,
            filter,
        };
        let mut targets = self.0.targets.borrow_mut();
        let found = targets.find(&rows, query, seen);
        let unit =
            |&index: &usize| ScriptValue::of(Unit::new(units[index].id, index, self.clone()));
        Ok(found.iter().map(unit).collect())
    }

    /// The nearest living target that `radius` from the edge of `of`'s body reaches in the map's
    /// metric, as a weapon's range does, that `filter` selects relative to it and `seen` lets by
    /// the view's columns and its row, by exact distance between centers, the lower stable id on
    /// a tie; `()` when there is none.
    pub fn nearest(
        &self,
        of: &Unit,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptValue> {
        debug_assert!(radius >= Num::ZERO, "a script's radius is not negative");
        let rows = self.rows();
        let of = rows.units()[of.row_index()?];
        let query = TargetQuery {
            team: of.team,
            from: of.position,
            shape: of.shape,
            radius,
            filter,
        };
        let metric = rows.metric();
        let mut nearest = None;
        self.0
            .targets
            .borrow_mut()
            .visit(&rows, query, seen, |entry| {
                let distance = metric
                    .offset(of.position, entry.body.position)
                    .length_squared_bits();
                let candidate = (distance, entry.id, entry.key);
                if nearest.is_none_or(|best| candidate < best) {
                    nearest = Some(candidate);
                }
            });
        Ok(nearest.map_or(ScriptValue::UNIT, |(_, id, at)| {
            ScriptValue::of(Unit::new(id, at, self.clone()))
        }))
    }
}

/// The view's units change every phase; a handle kept in a failure prints only that it is one.
impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("View")
    }
}
