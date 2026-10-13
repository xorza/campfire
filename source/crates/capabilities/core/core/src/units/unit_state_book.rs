use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_sim::Capability;
use campfire_values::NameTable;

use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::scripts::state_decl::StateType;
use crate::scripts::state_decl::synced_state_decl::{SyncTo, SyncedStateDecl};
use crate::scripts::state_value::StateValue;
use crate::units::unit_states::UnitStates;
use crate::units::unit_type::UnitType;
use crate::units::units_column::UnitsColumn;
use crate::units::view::View;

/// The script state each unit type declares, one run per type in the order of the types: each
/// field's name, type, first value and the clients it goes to. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the table.
#[derive(Resource, Debug, Clone, Default)]
pub struct UnitStateBook(Arc<NameTable<SyncedStateDecl>>);

/// Where a field of a unit type's state is among its values, in the order of their names, its
/// type, and its place in the component of its class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateField {
    pub(crate) at: usize,
    pub(crate) kind: StateType,
    pub(crate) place: StatePlace,
}

/// Where a field of a unit's state is held: the class of its `sync`, whose component holds it,
/// and its place among the values of that class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatePlace {
    pub sync: SyncTo,
    pub at: usize,
}

impl UnitStateBook {
    /// The book of `table`, one run per unit type.
    pub fn new(table: NameTable<SyncedStateDecl>) -> UnitStateBook {
        UnitStateBook(Arc::new(table))
    }

    /// The fields `unit_type` declares, in the order of their names; none for a type the book
    /// does not hold.
    pub(crate) fn fields(&self, unit_type: UnitType) -> &[SyncedStateDecl] {
        self.0.values(unit_type.index())
    }

    /// The field `name` of `unit_type`, when it declares one.
    pub fn field(&self, unit_type: UnitType, name: &str) -> Option<StateField> {
        let run = unit_type.index();
        let at = self.0.named(run, name)?;
        let fields = self.0.values(run);
        let sync = fields[at].sync;
        let place = StatePlace {
            sync,
            at: fields[..at]
                .iter()
                .filter(|field| field.sync == sync)
                .count(),
        };
        let kind = fields[at].decl.kind();
        Some(StateField { at, kind, place })
    }

    /// The state a unit of `unit_type` spawns with, each field at its first value.
    pub fn initial(&self, unit_type: UnitType) -> UnitStates {
        let mut states = UnitStates::default();
        for field in self.fields(unit_type) {
            states.push(field.sync, field.decl.initial().clone());
        }
        states
    }

    /// Whether `values` are the fields of class `sync` of a unit of `unit_type`: one for each
    /// such field it declares, each of the field's type, and at least one, as a unit holds no
    /// class its type declares no field of.
    pub(crate) fn holds(&self, unit_type: UnitType, sync: SyncTo, values: &[StateValue]) -> bool {
        if values.is_empty() {
            return false;
        }
        let mut fields = self
            .fields(unit_type)
            .iter()
            .filter(|field| field.sync == sync);
        let mut values = values.iter();
        loop {
            match (fields.next(), values.next()) {
                (None, None) => return true,
                (Some(field), Some(value)) if value.kind() == field.decl.kind() => {}
                _ => return false,
            }
        }
    }
}

impl Book for UnitStateBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        UnitsColumn::share(view, self.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::state_decl::{StateDecl, StateDefault};

    #[test]
    fn a_field_is_held_at_its_place_among_its_classes_fields() {
        // In the order of their names: `a` its owner's first, `b` no client's, `c` its owner's
        // second, of 7, and `d` its team's, a text.
        let field = |kind, default, sync| SyncedStateDecl {
            decl: StateDecl::new(kind, default).unwrap(),
            sync,
        };
        let fields = [
            ("a", field(StateType::Int, None, SyncTo::Owner)),
            ("b", field(StateType::Bool, None, SyncTo::None)),
            (
                "c",
                field(StateType::Int, Some(StateDefault::Int(7)), SyncTo::Owner),
            ),
            ("d", field(StateType::String, None, SyncTo::Team)),
        ];
        let mut table = NameTable::default();
        let unit_type = UnitType::new(u16::try_from(table.push(fields)).unwrap());
        let book = UnitStateBook::new(table);
        let place = |name| book.field(unit_type, name).map(|field| field.place);
        let at = |sync, at| Some(StatePlace { sync, at });
        assert_eq!(place("a"), at(SyncTo::Owner, 0));
        assert_eq!(place("b"), at(SyncTo::None, 0));
        assert_eq!(place("c"), at(SyncTo::Owner, 1));
        assert_eq!(place("d"), at(SyncTo::Team, 0));
        assert_eq!(place("e"), None);
        let c = book.field(unit_type, "c").unwrap();
        assert_eq!((c.at, c.kind), (2, StateType::Int));

        // It spawns with each class at its fields' first values, and none of all.
        let mut spawned = UnitStates::default();
        spawned.push(SyncTo::Owner, StateValue::Int(0));
        spawned.push(SyncTo::None, StateValue::Bool(false));
        spawned.push(SyncTo::Owner, StateValue::Int(7));
        spawned.push(SyncTo::Team, StateValue::Text(String::new()));
        assert_eq!(book.initial(unit_type), spawned);

        // A class holds exactly its fields, in order and of their types, and one it has no field
        // of is held by no unit.
        let ints = [StateValue::Int(1), StateValue::Int(2)];
        assert!(book.holds(unit_type, SyncTo::Owner, &ints));
        assert!(!book.holds(unit_type, SyncTo::Owner, &ints[..1]));
        assert!(!book.holds(
            unit_type,
            SyncTo::Owner,
            &[ints[0].clone(), StateValue::Bool(true)]
        ));
        assert!(book.holds(unit_type, SyncTo::Team, &[StateValue::Text(String::new())]));
        assert!(!book.holds(unit_type, SyncTo::All, &[]));
        assert!(!book.holds(unit_type, SyncTo::All, &ints[..1]));
    }
}
