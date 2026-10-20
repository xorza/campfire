use std::ops::Range;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_script::{
    AnyKeptRows, ApiError, Audience, Checked, ColumnRows, KeptRows, RowFill, RunMove, ScriptValue,
    StateField, StateSchema, StateValue, View, ViewColumn,
};
use campfire_values::{NameId, UnitType};

use crate::unit_state::{
    AudienceAll, AudienceNone, AudienceOwner, AudienceTeam, StateAudience, UnitState,
};
use crate::unit_state_book::UnitStateBook;

/// The parts of a unit the core reads into its row of script state: its type, which names the
/// class of each field, and its state of each class.
pub(crate) type StateRowReads = (
    Option<&'static UnitType>,
    Option<&'static UnitState<AudienceNone>>,
    Option<&'static UnitState<AudienceOwner>>,
    Option<&'static UnitState<AudienceTeam>>,
    Option<&'static UnitState<AudienceAll>>,
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
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    /// Takes the unit types' fields, as the load built them. A row holds its unit's values,
    /// which another book fills none of again.
    fn begin(&mut self, world: &World) -> bool {
        let book = world.resource::<UnitStateBook>();
        if !book.same(&self.book) {
            self.book = book.clone();
        }
        false
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
    /// the order of their names as the type's schema places each field.
    fn push(&mut self, unit_type: Option<UnitType>, state: StateParts<'_, '_>) {
        fn class<S: StateAudience>(state: Option<&UnitState<S>>) -> &[StateValue] {
            state.map_or(&[][..], UnitState::values)
        }
        let (_, none, owner, team, all) = state;
        let rows = self.rows.now_mut();
        let start = len(rows.values.len());
        if let Some(schema) = unit_type.and_then(|unit_type| self.book.schema(unit_type)) {
            let classes = [
                (Audience::None, class(none)),
                (Audience::Owner, class(owner)),
                (Audience::Team, class(team)),
                (Audience::All, class(all)),
            ];
            debug_assert!(
                classes
                    .iter()
                    .all(|&(audience, values)| schema.holds(audience, values)),
                "a unit holds exactly the fields its type declares"
            );
            for place in schema.places() {
                let values = classes[place.audience.index()].1;
                rows.values.push(values[place.at].clone());
            }
        }
        rows.rows.push(start..len(rows.values.len()));
    }

    /// What `read` gives of the schema of `unit_type`, a type of the match.
    pub(crate) fn schema<R>(
        view: &View,
        unit_type: UnitType,
        read: impl FnOnce(&StateSchema) -> R,
    ) -> R {
        view.column(|column: &Self| {
            read(
                column
                    .book
                    .schema(unit_type)
                    .expect("a unit's type is the match's"),
            )
        })
        .expect("a view of units has their state")
    }

    /// The field `name` of a unit of `unit_type`; one its type does not declare fails the call.
    pub(crate) fn field(view: &View, unit_type: UnitType, name: &str) -> Checked<StateField> {
        let found = Self::schema(view, unit_type, |schema| schema.field(name));
        Ok(found.ok_or_else(|| ApiError::UnknownState.fail())?)
    }

    /// The value of `field` of the unit in row `row`, as a script reads it.
    pub(crate) fn read(view: &View, row: usize, field: StateField) -> ScriptValue {
        let value = view
            .column(|column: &Self| {
                let rows = column.rows.now();
                rows.values[rows.rows[row].start as usize + field.index()].clone()
            })
            .expect("a view of units has their state");
        value.to_value(view)
    }

    /// The first value of `field` of `unit_type`, as a script reads it.
    pub(crate) fn initial(view: &View, unit_type: UnitType, field: StateField) -> ScriptValue {
        let value = Self::schema(view, unit_type, |schema| {
            schema.initial()[field.index()].clone()
        });
        value.to_value(view)
    }

    /// Writes `value` to `field` of the unit in row `row`, as a call's write applies to the
    /// unit's state too, so a later call of the stage reads it.
    pub(crate) fn write(view: &View, row: usize, field: StateField, value: StateValue) {
        view.write_rows(|column: &mut Self| {
            let rows = column.rows.now_mut();
            let start = rows.rows[row].start as usize;
            rows.values[start + field.index()] = value;
        });
    }

    /// Adds a unit's script state to the view's column of it.
    pub(super) fn fill_state(
        state: ROQueryItem<'_, '_, StateRowReads>,
        fill: &mut RowFill<'_, Self>,
    ) {
        fill.column.push(state.0.copied(), state);
    }
}

fn len(count: usize) -> u32 {
    u32::try_from(count).expect("a view's state values fit u32")
}
