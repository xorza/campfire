use campfire_values::{NameId, Names};

use crate::scripts::state_decl::synced_state_decl::SyncTo;
use crate::scripts::state_decl::{StateDecl, StateType};
use crate::scripts::state_value::StateValue;

/// The fields of one declared script state, the mode's, a unit type's or a modifier's: each
/// field's name, found once, its first value, whose type is the field's, and where its value is
/// held. The fields are in the order of their names. Package data, built once at the load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateSchema {
    names: Names<StateField>,
    /// The first value of each field.
    initial: Box<[StateValue]>,
    places: Box<[StatePlace]>,
}

/// A field of a script state: its place among the state's fields, in the order of their names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateField(u32);

/// Where a field's value is held: the class of its `sync`, whose values a component holds, and
/// its place among the values of that class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatePlace {
    pub sync: SyncTo,
    pub at: usize,
}

impl NameId for StateField {
    type Scope = ();
    const LIMIT: usize = u32::MAX as usize;

    fn at(index: usize) -> StateField {
        StateField(u32::try_from(index).expect("a field below the limit"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl StateField {
    /// Its place among the state's fields.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl StateSchema {
    /// The schema of `fields`, each name with the class it syncs to and its declaration, in the
    /// order of their names, as data's tables give them.
    pub fn new<'a>(
        fields: impl IntoIterator<Item = (&'a str, SyncTo, &'a StateDecl)>,
    ) -> StateSchema {
        let mut names = Names::default();
        let (mut initial, mut places) = (Vec::new(), Vec::new());
        let mut counts = [0; 4];
        let mut previous: Option<&str> = None;
        for (name, sync, decl) in fields {
            debug_assert!(
                previous.is_none_or(|previous| previous < name),
                "the fields come in the order of their names"
            );
            previous = Some(name);
            names.add(name);
            let class = &mut counts[sync.index()];
            places.push(StatePlace { sync, at: *class });
            *class += 1;
            initial.push(decl.initial().clone());
        }
        StateSchema {
            names,
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
        self.initial[field.index()].kind()
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

    /// Whether `values` are the values of class `sync`: one for each field of the class, in the
    /// order of their names, each of its field's type.
    pub fn holds(&self, sync: SyncTo, values: &[StateValue]) -> bool {
        let fields = self.initial.iter().zip(&self.places);
        let mut kinds = fields
            .filter(|(_, place)| place.sync == sync)
            .map(|(initial, _)| initial.kind());
        let fits = values
            .iter()
            .all(|value| kinds.next() == Some(value.kind()));
        fits && kinds.next().is_none()
    }

    /// The first value of each field of class `sync`, in the order of their names.
    pub fn initial_of(&self, sync: SyncTo) -> impl Iterator<Item = &StateValue> {
        let fields = self.initial.iter().zip(&self.places);
        fields
            .filter(move |(_, place)| place.sync == sync)
            .map(|(initial, _)| initial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::state_decl::StateDefault;
    use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;

    #[test]
    fn a_field_is_found_by_name_with_its_type_first_value_and_place_in_its_class() {
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
        let fields = fields.iter();
        let schema = StateSchema::new(fields.map(|(name, field)| (*name, field.sync, &field.decl)));
        let place = |name| schema.field(name).map(|field| schema.place(field));
        let at = |sync, at| Some(StatePlace { sync, at });
        assert_eq!(place("a"), at(SyncTo::Owner, 0));
        assert_eq!(place("b"), at(SyncTo::None, 0));
        assert_eq!(place("c"), at(SyncTo::Owner, 1));
        assert_eq!(place("d"), at(SyncTo::Team, 0));
        assert_eq!(place("e"), None);
        let c = schema.field("c").unwrap();
        assert_eq!((c.index(), schema.kind(c)), (2, StateType::Int));
        assert_eq!(schema.len(), 4);
        let owner: Vec<_> = schema.initial_of(SyncTo::Owner).collect();
        assert_eq!(owner, [&StateValue::Int(0), &StateValue::Int(7)]);
        assert_eq!(schema.initial_of(SyncTo::All).count(), 0);

        // A class holds exactly its fields, in order and of their types; one with no field holds
        // no value.
        let ints = [StateValue::Int(1), StateValue::Int(2)];
        assert!(schema.holds(SyncTo::Owner, &ints));
        assert!(!schema.holds(SyncTo::Owner, &ints[..1]));
        let mixed = [ints[0].clone(), StateValue::Bool(true)];
        assert!(!schema.holds(SyncTo::Owner, &mixed));
        assert!(schema.holds(SyncTo::Team, &[StateValue::Text("".into())]));
        assert!(schema.holds(SyncTo::All, &[]));
        assert!(!schema.holds(SyncTo::All, &ints[..1]));
    }
}
