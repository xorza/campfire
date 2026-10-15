use campfire_sim::StableId;

use crate::scripts::view::View;
use crate::scripts::view::unit_row::UnitRow;

/// A unit as a script holds it, `Unit` in scripts: its values as the view read them.
#[derive(Debug, Clone)]
pub struct Unit {
    pub id: StableId,
    /// Its place among the rows of its view, which does not change within a call.
    row: usize,
    view: View,
}

impl Unit {
    /// Only the view makes a handle, of a unit it read: `View::unit` for any other code.
    pub(crate) const fn new(id: StableId, row: usize, view: View) -> Unit {
        Unit { id, row, view }
    }

    pub const fn view(&self) -> &View {
        &self.view
    }

    /// The unit's place among its view's rows.
    pub const fn row_index(&self) -> usize {
        self.row
    }

    /// What `read` reads of the unit's row, in place. A handle comes only from its view's rows,
    /// and lives only within a call.
    pub fn read<R>(&self, read: impl FnOnce(&UnitRow) -> R) -> R {
        self.view.read_row(self.row, read)
    }
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
                .map(|unit| unit.try_cast::<Unit>().unwrap().id)
                .collect()
        }
    }
}
