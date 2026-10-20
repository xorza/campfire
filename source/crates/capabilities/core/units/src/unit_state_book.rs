use std::slice;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::{Audience, MatchNames, StateSchema, StateValue};
use campfire_values::{NameId, UnitType};

/// The script state each unit type declares, a schema for each type in the order of the types:
/// each field's name, type, first value and the clients it goes to. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the schemas.
#[derive(Resource, Debug, Clone)]
pub struct UnitStateBook(Arc<[StateSchema]>);

impl UnitStateBook {
    /// The schema of `unit_type`; none for a type the book does not hold.
    pub fn schema(&self, unit_type: UnitType) -> Option<&StateSchema> {
        self.0.get(unit_type.index())
    }

    /// Whether `values` are the fields of class `audience` of a unit of `unit_type`: one for each
    /// such field it declares, each of the field's type, and at least one, as a unit holds no
    /// class its type declares no field of.
    pub(crate) fn holds(
        &self,
        unit_type: UnitType,
        audience: Audience,
        values: &[StateValue],
    ) -> bool {
        let schema = self.schema(unit_type);
        !values.is_empty() && schema.is_some_and(|schema| schema.holds(audience, values))
    }

    /// Whether `other` is this book as one load made it, not a book of another load.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Each type's schema, in the order of the types.
    pub fn iter(&self) -> slice::Iter<'_, StateSchema> {
        self.0.iter()
    }
}

impl Default for UnitStateBook {
    fn default() -> Self {
        Self(Arc::from([]))
    }
}

impl FromIterator<StateSchema> for UnitStateBook {
    fn from_iter<I: IntoIterator<Item = StateSchema>>(schemas: I) -> Self {
        Self(schemas.into_iter().collect())
    }
}

impl<const N: usize> From<[StateSchema; N]> for UnitStateBook {
    fn from(schemas: [StateSchema; N]) -> Self {
        schemas.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a UnitStateBook {
    type Item = &'a StateSchema;
    type IntoIter = slice::Iter<'a, StateSchema>;

    fn into_iter(self) -> slice::Iter<'a, StateSchema> {
        self.iter()
    }
}

impl Book for UnitStateBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(test)]
mod tests {
    use campfire_script::{StateDecl, StateDefault, StateFieldDecl, StateType};

    use super::*;
    use crate::unit_states::UnitStates;

    #[test]
    fn a_unit_spawns_with_its_types_first_values_and_holds_only_classes_it_declares() {
        // In the order of their names: `a` its owner's first, `b` no client's, `c` its owner's
        // second, of 7, and `d` its team's, a text.
        let field = |kind, default, audience| StateFieldDecl {
            decl: StateDecl::new(kind, default).unwrap(),
            audience,
        };
        let fields = [
            ("a", field(StateType::Int, None, Audience::Owner)),
            ("b", field(StateType::Bool, None, Audience::None)),
            (
                "c",
                field(StateType::Int, Some(StateDefault::Int(7)), Audience::Owner),
            ),
            ("d", field(StateType::String, None, Audience::Team)),
        ];
        let fields = fields.iter();
        let schema =
            StateSchema::new(fields.map(|(name, field)| (*name, field.audience, &field.decl)));
        let book = UnitStateBook::from([StateSchema::default(), schema]);
        let (bare, unit_type) = (UnitType::new(0), UnitType::new(1));

        // It spawns with each class at its fields' first values, and none of all.
        let mut spawned = UnitStates::default();
        spawned.push(Audience::Owner, StateValue::Int(0));
        spawned.push(Audience::None, StateValue::Bool(false));
        spawned.push(Audience::Owner, StateValue::Int(7));
        spawned.push(Audience::Team, StateValue::Text("".into()));
        assert_eq!(UnitStates::initial(&book, unit_type), spawned);
        assert_eq!(UnitStates::initial(&book, bare), UnitStates::default());
        assert_eq!(
            UnitStates::initial(&book, UnitType::new(2)),
            UnitStates::default()
        );

        // A unit holds no class its type has no field of, though the schema holds it empty.
        let int = [StateValue::Int(1), StateValue::Int(2)];
        assert!(book.holds(unit_type, Audience::Owner, &int));
        assert!(!book.holds(unit_type, Audience::Owner, &int[..1]));
        assert!(!book.holds(unit_type, Audience::All, &[]));
        assert!(!book.holds(bare, Audience::None, &[]));
        assert!(!book.holds(UnitType::new(2), Audience::Owner, &int));
    }
}
