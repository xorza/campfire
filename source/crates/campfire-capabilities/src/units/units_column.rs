use std::ops::Range;

use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_value::StateValue;
use crate::units::kept_rows::{ColumnRows, KeptRows, RunMove};
use crate::units::script_view::View;
use crate::units::unit_state::UnitState;
use crate::units::unit_state_book::{StateField, UnitStateBook};
use crate::units::unit_type::UnitType;
use crate::units::view_column::ViewColumn;

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

impl StateRows {
    fn push(&mut self, values: &[StateValue]) {
        let start = len(self.values.len());
        self.values.extend_from_slice(values);
        self.rows.push(start..len(self.values.len()));
    }
}

impl UnitsColumn {
    /// Adds the row of a unit with `state`, none for one whose type declares no field.
    pub(crate) fn push(&mut self, state: Option<&UnitState>) {
        let values = state.map_or(&[][..], UnitState::values);
        self.rows.now_mut().push(values);
    }

    /// Shares the unit types' fields, as the load built them.
    pub(crate) fn share(view: &View, book: UnitStateBook) {
        view.column_mut(|column: &mut UnitsColumn| column.book = book);
    }

    /// The field `name` of a unit of `unit_type`; one its type does not declare fails the call.
    pub(crate) fn field(
        view: &View,
        unit_type: Option<UnitType>,
        name: &str,
    ) -> Checked<StateField> {
        let found = view
            .column(|column: &UnitsColumn| {
                unit_type.and_then(|unit_type| column.book.field_named(unit_type, name))
            })
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
            .column(|column: &UnitsColumn| column.book.fields(unit_type)[at].decl.initial.clone())
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
}

fn len(count: usize) -> u32 {
    u32::try_from(count).expect("a view's state values fit u32")
}
