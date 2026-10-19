use campfire_sim::StableId;
use campfire_values::UnitType;

use crate::scripts::api_holder::ScriptHandle;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::view::View;
use crate::scripts::view::unit_row::UnitRow;

/// A unit as a script holds it, `Unit` in scripts: its values as the view read them, or none yet
/// for a unit the call creates.
#[derive(Debug, Clone)]
pub struct Unit {
    pub id: StableId,
    at: UnitAt,
    view: View,
}

/// Where a unit's values are: its place among the rows of its view, which does not change within
/// a call, or none for a unit the call creates, of its type, which spawns as the call applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitAt {
    Row(usize),
    Created(UnitType),
}

impl Unit {
    /// Only the view makes a handle of a unit it read: `View::unit` for any other code.
    pub(crate) const fn new(id: StableId, row: usize, view: View) -> Self {
        Self {
            id,
            at: UnitAt::Row(row),
            view,
        }
    }

    /// The unit `id` of `unit_type` that the call creates, which spawns as it applies.
    pub const fn created(id: StableId, unit_type: UnitType, view: View) -> Self {
        Self {
            id,
            at: UnitAt::Created(unit_type),
            view,
        }
    }

    pub const fn view(&self) -> &View {
        &self.view
    }

    pub const fn at(&self) -> UnitAt {
        self.at
    }

    /// The unit's place among its view's rows; a unit the call creates has none yet, and fails
    /// the call.
    pub fn row_index(&self) -> Checked<usize> {
        match self.at {
            UnitAt::Row(row) => Ok(row),
            UnitAt::Created(_) => Err(ApiError::Unspawned.fail().into()),
        }
    }

    /// What `read` reads of the unit's row, in place; a unit the call creates has none yet, and
    /// fails the call. A handle comes only from its view's rows or the call's spawns, and lives
    /// only within a call.
    pub fn read<R>(&self, read: impl FnOnce(&UnitRow) -> R) -> Checked<R> {
        Ok(self.view.read_row(self.row_index()?, read))
    }
}

impl ScriptHandle for Unit {
    const NAME: &'static str = "Unit";
    const TITLE: &'static str = "Unit";
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::scripts::script_value::ScriptValue;
    use crate::scripts::script_value::script_list::ScriptList;
    use crate::scripts::view::unit::Unit;

    impl Unit {
        /// The stable ids of `value`, a unit, a list of units, or `()` for none.
        pub fn ids(value: ScriptValue) -> Vec<StableId> {
            let units = match value.clone().try_cast::<ScriptList>() {
                Some(units) => units,
                None if value.is_unit() => ScriptList::default(),
                None => ScriptList::from([value]),
            };
            units
                .into_iter()
                .map(|unit| unit.try_cast::<Self>().unwrap().id)
                .collect()
        }
    }
}
