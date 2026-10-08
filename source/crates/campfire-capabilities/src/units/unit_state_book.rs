use std::sync::Arc;

use bevy_ecs::resource::Resource;

use crate::scripts::state_decl::StateType;
use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::scripts::state_value::StateValue;
use crate::units::unit_state::UnitState;
use crate::units::unit_type::UnitType;
use crate::values::name_table::NameTable;

/// The script state each unit type declares, one run per type in the order of the types: each
/// field's name, type, first value and the clients it goes to. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the table.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct UnitStateBook(Arc<NameTable<SyncedStateDecl>>);

/// Where a field of a unit type's state is among its values, and its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateField {
    pub(crate) at: usize,
    pub(crate) kind: StateType,
}

impl UnitStateBook {
    /// The book of `table`, one run per unit type.
    pub(crate) fn new(table: NameTable<SyncedStateDecl>) -> UnitStateBook {
        UnitStateBook(Arc::new(table))
    }

    /// The fields `unit_type` declares, in the order of their names; none for a type the book
    /// does not hold.
    pub(crate) fn fields(&self, unit_type: UnitType) -> &[SyncedStateDecl] {
        self.0.values(unit_type.index())
    }

    /// The field `name` of `unit_type`, when it declares one.
    pub(crate) fn field_named(&self, unit_type: UnitType, name: &str) -> Option<StateField> {
        let run = unit_type.index();
        let at = self.0.named(run, name)?;
        let kind = self.0.values(run)[at].decl.kind();
        Some(StateField { at, kind })
    }

    /// The state a unit of `unit_type` spawns with, each field at its first value; none for a
    /// type that declares no field.
    pub(crate) fn initial(&self, unit_type: UnitType) -> Option<UnitState> {
        let fields = self.fields(unit_type);
        if fields.is_empty() {
            return None;
        }
        Some(UnitState::new(
            fields
                .iter()
                .map(|field| field.decl.initial().clone())
                .collect(),
        ))
    }

    /// Whether `values` are the state of a unit of `unit_type`: one for each field it declares,
    /// each of the field's type.
    pub(crate) fn holds(&self, unit_type: UnitType, values: &[StateValue]) -> bool {
        let fields = self.fields(unit_type);
        fields.len() == values.len()
            && fields
                .iter()
                .zip(values)
                .all(|(field, value)| value.kind() == field.decl.kind())
    }
}
