use std::ops::Range;
use std::slice::Iter;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_decl::synced_state_decl::SyncTo;
use crate::scripts::state_value::StateValue;
use crate::units::kept_rows::{ColumnRows, KeptRows, RunMove};
use crate::units::row_fill::RowFill;
use crate::units::unit_state::{StateSync, SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::{StateField, UnitStateBook};
use crate::units::unit_type::UnitType;
use crate::units::view::View;
use crate::units::view_column::ViewColumn;

/// The parts of a unit the core reads into its row of script state: its type, which names the
/// class of each field, and its state of each class.
pub(super) type StateRowReads = (
    Option<&'static UnitType>,
    Option<&'static UnitState<SyncNone>>,
    Option<&'static UnitState<SyncOwner>>,
    Option<&'static UnitState<SyncTeam>>,
    Option<&'static UnitState<SyncAll>>,
);

/// A row's parts of `StateRowReads`, as the view reads them.
type StateParts<'w, 's> = ROQueryItem<'w, 's, StateRowReads>;

/// What the core adds to the script view for units' script state: the fields each unit type
/// declares, and each unit's values, a run a row, as the view read them and as the calls of the
/// stage since wrote them.
#[derive(Debug, Default)]
pub(crate) struct UnitsColumn {
    book: UnitStateBook,
    rows: KeptRows<StateRows>,
}

/// The rows of one read of the units column: each unit's run of values.
#[derive(Debug, Default, PartialEq)]
struct StateRows {
    rows: Vec<Range<u32>>,
    values: Vec<StateValue>,
}

impl ViewColumn for UnitsColumn {
    fn begin(&mut self, _: &World) -> bool {
        self.rows.begin();
        false
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.rows.keep(rows);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
    }
}

impl ColumnRows for StateRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.values.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let (first, end) = (from.rows[rows.start].start, from.rows[rows.end - 1].end);
        let moved = RunMove::new(first, self.values.len());
        self.rows
            .extend(from.rows[rows].iter().map(|run| moved.of(run)));
        self.values
            .extend_from_slice(&from.values[first as usize..end as usize]);
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

impl UnitsColumn {
    /// Adds the row of a unit of `unit_type` with `state`, its fields of each class, merged into
    /// the order of their names as the book gives each field's class.
    fn push(&mut self, unit_type: Option<UnitType>, state: StateParts<'_, '_>) {
        fn class<S: StateSync>(state: Option<&UnitState<S>>) -> Iter<'_, StateValue> {
            state.map_or(&[][..], UnitState::values).iter()
        }
        let (_, none, owner, team, all) = state;
        let mut classes = [class(none), class(owner), class(team), class(all)];
        let rows = self.rows.now_mut();
        let start = len(rows.values.len());
        let fields = unit_type.map_or(&[][..], |unit_type| self.book.fields(unit_type));
        for field in fields {
            let at = match field.sync {
                SyncTo::None => 0,
                SyncTo::Owner => 1,
                SyncTo::Team => 2,
                SyncTo::All => 3,
            };
            let value = classes[at]
                .next()
                .expect("a unit holds each field its type declares");
            rows.values.push(value.clone());
        }
        debug_assert!(
            classes.iter_mut().all(|class| class.next().is_none()),
            "a unit holds only the fields its type declares"
        );
        rows.rows.push(start..len(rows.values.len()));
    }

    /// Shares the unit types' fields, as the load built them.
    pub(crate) fn share(view: &View, book: UnitStateBook) {
        view.column_mut(|column: &mut UnitsColumn| column.book = book);
    }

    /// The field `name` of a unit of `unit_type`; one its type does not declare fails the call.
    pub(crate) fn field(view: &View, unit_type: UnitType, name: &str) -> Checked<StateField> {
        let found = view
            .column(|column: &UnitsColumn| column.book.field(unit_type, name))
            .expect("a view of units has their state");
        Ok(found.ok_or_else(|| ApiError::UnknownState.fail())?)
    }

    /// The value at `at` of the unit in row `row`, as a script reads it.
    pub(crate) fn read(view: &View, row: usize, at: usize) -> Dynamic {
        let value = view
            .column(|column: &UnitsColumn| {
                let rows = column.rows.now();
                rows.values[rows.rows[row].start as usize + at].clone()
            })
            .expect("a view of units has their state");
        value.to_dynamic(view)
    }

    /// The default of the field at `at` of `unit_type`, as a script reads it.
    pub(crate) fn initial(view: &View, unit_type: UnitType, at: usize) -> Dynamic {
        let value = view
            .column(|column: &UnitsColumn| column.book.fields(unit_type)[at].decl.initial().clone())
            .expect("a view of units has their state");
        value.to_dynamic(view)
    }

    /// Writes `value` at `at` of the unit in row `row`, as a call's write applies to the unit's
    /// state too, so a later call of the stage reads it.
    pub(crate) fn write(view: &View, row: usize, at: usize, value: StateValue) {
        view.write_rows(|column: &mut UnitsColumn| {
            let rows = column.rows.now_mut();
            let start = rows.rows[row].start as usize;
            rows.values[start + at] = value;
        });
    }

    /// Adds a unit's script state to the view's column of it.
    pub(super) fn fill_state(
        state: ROQueryItem<'_, '_, StateRowReads>,
        fill: &mut RowFill<'_, UnitsColumn>,
    ) {
        fill.column.push(state.0.copied(), state);
    }
}

fn len(count: usize) -> u32 {
    u32::try_from(count).expect("a view's state values fit u32")
}
