use std::ops::Range;

use campfire_script::rhai::Dynamic;

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_value::StateValue;
use crate::units::script_view::View;
use crate::units::unit_state::UnitState;
use crate::units::unit_state_book::{StateField, UnitStateBook};
use crate::units::unit_type::UnitType;
use crate::units::view_column::ViewColumn;

/// What the core adds to the script view for units' script state: the fields each unit type
/// declares, and each unit's values, a run a row, as the view read them and as the calls of the
/// stage since wrote them.
#[derive(Debug, Default)]
pub(crate) struct UnitStateColumn {
    book: UnitStateBook,
    rows: Vec<Range<u32>>,
    values: Vec<StateValue>,
}

impl ViewColumn for UnitStateColumn {
    fn clear(&mut self) {
        self.rows.clear();
        self.values.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl UnitStateColumn {
    /// Adds the row of a unit with `state`, none for one whose type declares no field.
    pub(crate) fn push(&mut self, state: Option<&UnitState>) {
        let start = len(self.values.len());
        if let Some(state) = state {
            self.values.extend_from_slice(state.values());
        }
        self.rows.push(start..len(self.values.len()));
    }

    /// Shares the unit types' fields, as the load built them.
    pub(crate) fn share(view: &View, book: UnitStateBook) {
        view.column_mut(|column: &mut UnitStateColumn| column.book = book);
    }

    /// The field `name` of a unit of `unit_type`; one its type does not declare fails the call.
    pub(crate) fn field(
        view: &View,
        unit_type: Option<UnitType>,
        name: &str,
    ) -> Checked<StateField> {
        let found = view
            .column(|column: &UnitStateColumn| {
                unit_type.and_then(|unit_type| column.book.field_named(unit_type, name))
            })
            .expect("a view of units has their state");
        Ok(found.ok_or_else(|| ApiError::UnknownState.fail())?)
    }

    /// The value at `at` of the unit in row `row`, as a script reads it.
    pub(crate) fn read(view: &View, row: usize, at: usize) -> Dynamic {
        let value = view
            .column(|column: &UnitStateColumn| {
                let run = &column.rows[row];
                column.values[run.start as usize + at].clone()
            })
            .expect("a view of units has their state");
        value.to_dynamic(view)
    }

    /// Writes `value` at `at` of the unit in row `row`, as a call's write applies, so a later
    /// call of the stage reads it.
    pub(crate) fn write(view: &View, row: usize, at: usize, value: StateValue) {
        view.column_mut(|column: &mut UnitStateColumn| {
            let run = column.rows[row].clone();
            column.values[run.start as usize + at] = value;
        });
    }
}

fn len(count: usize) -> u32 {
    u32::try_from(count).expect("a view's state values fit u32")
}
