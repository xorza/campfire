use campfire_values::{NameId, Names};

use crate::scripts::error::Checked;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::state_decl::StateDecl;
use crate::scripts::state_decl::state_field_decl::Audience;
use crate::scripts::state_type::StateType;
use crate::scripts::state_value::StateValue;

/// The fields of one declared script state, the mode's, a unit type's or a modifier's: each field's
/// name, found once, its declaration, its first value, and where its value is held. The fields are
/// in the order of their names. Package data, built once at the load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateSchema {
    names: Names<StateField>,
    decls: Box<[StateDecl]>,
    /// The first value of each field.
    initial: Box<[StateValue]>,
    places: Box<[StatePlace]>,
}

/// A field of a script state: its place among the state's fields, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateField(u32);

/// Where a field's value is held: the class of its `audience`, whose values a component holds, and
/// its place among the values of that class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatePlace {
    pub audience: Audience,
    pub at: usize,
}

impl NameId for StateField {
    type Scope = ();
    const LIMIT: usize = u32::MAX as usize;

    fn at(index: usize) -> Self {
        Self(u32::try_from(index).expect("a field below the limit"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl StateSchema {
    /// The schema of `fields`, each name with the class it goes to and its declaration, in the
    /// order of their names, as data's tables give them.
    pub fn new<'a>(fields: impl IntoIterator<Item = (&'a str, Audience, &'a StateDecl)>) -> Self {
        let mut names = Names::default();
        let (mut decls, mut initial, mut places) = (Vec::new(), Vec::new(), Vec::new());
        let mut counts = [0; Audience::ALL.len()];
        let mut previous: Option<&str> = None;
        for (name, audience, decl) in fields {
            debug_assert!(
                previous.is_none_or(|previous| previous < name),
                "the fields come in the order of their names"
            );
            previous = Some(name);
            names.add(name);
            let class = &mut counts[audience.index()];
            places.push(StatePlace {
                audience,
                at: *class,
            });
            *class += 1;
            initial.push(decl.initial().clone());
            decls.push(decl.clone());
        }
        Self {
            names,
            decls: decls.into(),
            initial: initial.into(),
            places: places.into(),
        }
    }

    /// The field `name`, when the state declares one.
    pub fn field(&self, name: &str) -> Option<StateField> {
        self.names.named(name)
    }

    /// The type of `field`.
    pub fn kind(&self, field: StateField) -> StateType {
        self.decls[field.index()].kind()
    }

    /// A script's value as a value of `field`; an error when it is not one.
    pub fn take(&self, field: StateField, value: &ScriptValue) -> Checked<StateValue> {
        self.decls[field.index()].take(value)
    }

    /// Where `field`'s value is held.
    pub fn place(&self, field: StateField) -> StatePlace {
        self.places[field.index()]
    }

    /// The first value of each field, in the order of their names.
    pub fn initial(&self) -> &[StateValue] {
        &self.initial
    }

    /// Where each field's value is held, in the order of their names.
    pub fn places(&self) -> &[StatePlace] {
        &self.places
    }

    /// How many fields the state declares.
    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Whether `values` are the values of class `audience`: one for each field of the class, in the
    /// order of their names, each a value of its field.
    pub fn holds(&self, audience: Audience, values: &[StateValue]) -> bool {
        let fields = self.decls.iter().zip(&self.places);
        let mut decls = fields
            .filter(|(_, place)| place.audience == audience)
            .map(|(decl, _)| decl);
        let fits = values
            .iter()
            .all(|value| decls.next().is_some_and(|decl| decl.holds(value)));
        fits && decls.next().is_none()
    }

    /// The first value of each field of class `audience`, in the order of their names.
    pub fn initial_of(&self, audience: Audience) -> impl Iterator<Item = &StateValue> {
        let fields = self.initial.iter().zip(&self.places);
        fields
            .filter(move |(_, place)| place.audience == audience)
            .map(|(initial, _)| initial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::state_decl::StateDefault;
    use crate::scripts::state_decl::state_field_decl::StateFieldDecl;

    #[test]
    fn a_field_is_found_by_name_with_its_type_first_value_and_place_in_its_class() {
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
        let place = |name| schema.field(name).map(|field| schema.place(field));
        let at = |audience, at| Some(StatePlace { audience, at });
        assert_eq!(place("a"), at(Audience::Owner, 0));
        assert_eq!(place("b"), at(Audience::None, 0));
        assert_eq!(place("c"), at(Audience::Owner, 1));
        assert_eq!(place("d"), at(Audience::Team, 0));
        assert_eq!(place("e"), None);
        let c = schema.field("c").unwrap();
        assert_eq!((c.index(), schema.kind(c)), (2, StateType::Int));
        assert_eq!(schema.len(), 4);
        let owner: Vec<_> = schema.initial_of(Audience::Owner).collect();
        assert_eq!(owner, [&StateValue::Int(0), &StateValue::Int(7)]);
        assert_eq!(schema.initial_of(Audience::All).count(), 0);

        // A class holds exactly its fields, in order and of their types; one with no field holds
        // no value.
        let ints = [StateValue::Int(1), StateValue::Int(2)];
        assert!(schema.holds(Audience::Owner, &ints));
        assert!(!schema.holds(Audience::Owner, &ints[..1]));
        let mixed = [ints[0].clone(), StateValue::Bool(true)];
        assert!(!schema.holds(Audience::Owner, &mixed));
        assert!(schema.holds(Audience::Team, &[StateValue::Text("".into())]));
        assert!(schema.holds(Audience::All, &[]));
        assert!(!schema.holds(Audience::All, &ints[..1]));
    }
}
