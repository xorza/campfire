use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_script::MatchNames;
use campfire_values::{NameId, UnitType};

use crate::book::Book;
use crate::per_type::PerType;

/// A value for some of the match's unit types, by type: a capability's package data about each
/// type that has it, such as its AI or a part its units spawn with. A clone shares the values,
/// so a reader keeps the book itself, not a copy of what it holds; `ByTypeBuilder` builds it.
#[derive(Resource, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct ByType<T: Send + Sync + 'static> {
    entries: Arc<[Option<T>]>,
}

/// A `ByType` as it is built: each type's value set once, in any order.
#[derive(Debug)]
pub struct ByTypeBuilder<T: Send + Sync + 'static> {
    entries: Vec<Option<T>>,
}

impl<T: Send + Sync + 'static> ByType<T> {
    pub fn get(&self, unit_type: UnitType) -> Option<&T> {
        self.entries.get(unit_type.index())?.as_ref()
    }

    /// The values of the types that have one, by type.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().flatten()
    }

    /// Each type that has a value, with it, by type.
    pub fn iter(&self) -> impl Iterator<Item = (UnitType, &T)> {
        let entries = self.entries.iter().enumerate();
        entries.filter_map(|(at, value)| Some((UnitType::at(at), value.as_ref()?)))
    }

    /// Whether no type has a value.
    pub fn is_empty(&self) -> bool {
        self.values().next().is_none()
    }

    /// The book as it is built again, its values moved out, as an extension of the book changes
    /// it before the book is shared.
    pub fn reopen(mut self) -> ByTypeBuilder<T> {
        let entries = Arc::get_mut(&mut self.entries).expect("a book changes before it is shared");
        ByTypeBuilder {
            entries: entries.iter_mut().map(Option::take).collect(),
        }
    }
}

impl<T: Send + Sync + 'static> ByTypeBuilder<T> {
    /// Gives `unit_type` its `value`, which it has none of yet.
    pub fn set(&mut self, unit_type: UnitType, value: T) {
        let index = unit_type.index();
        if self.entries.len() <= index {
            self.entries.resize_with(index + 1, || None);
        }
        assert!(self.entries[index].is_none(), "a unit type has one value");
        self.entries[index] = Some(value);
    }

    /// Takes `unit_type`'s value out.
    pub fn take(&mut self, unit_type: UnitType) -> Option<T> {
        self.entries.get_mut(unit_type.index())?.take()
    }

    /// Gives `unit_type` `value`, when it is some, as `set` does.
    pub fn set_some(&mut self, unit_type: UnitType, value: Option<T>) {
        if let Some(value) = value {
            self.set(unit_type, value);
        }
    }

    /// The book of the values set.
    pub fn build(self) -> ByType<T> {
        ByType {
            entries: Arc::from(self.entries),
        }
    }
}

impl<T: Send + Sync + 'static> Clone for ByType<T> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

impl<T: Send + Sync + 'static> Default for ByType<T> {
    fn default() -> Self {
        Self {
            entries: Arc::new([]),
        }
    }
}

impl<T: Send + Sync + 'static> Default for ByTypeBuilder<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T: PerType> Book for ByType<T> {
    const SCRIPTS_ONLY: bool = T::SCRIPTS_ONLY;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::mem;

    use campfire_values::UnitType;

    use crate::by_type::ByType;

    impl<T: Send + Sync + 'static> ByType<T> {
        /// Gives `unit_type` its `value` in a built book, which no clone shares yet, as a test
        /// gives a type it adds its value: the book is built again.
        pub fn set(&mut self, unit_type: UnitType, value: T) {
            let mut built = mem::take(self).reopen();
            built.set(unit_type, value);
            *self = built.build();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_has_its_value_or_none() {
        let mut built = ByTypeBuilder::default();
        built.set(UnitType::new(2), 'c');
        built.set(UnitType::new(0), 'a');
        // None gives a type no value, within the table or past its end.
        built.set_some(UnitType::new(1), None);
        built.set_some(UnitType::new(4), None);
        let table = built.build();
        let read = [0, 1, 2, 3, 4].map(|index| table.get(UnitType::new(index)).copied());
        assert_eq!(read, [Some('a'), None, Some('c'), None, None]);
        assert_eq!(table.values().collect::<Vec<_>>(), [&'a', &'c']);
        // An extension reopens the book before it is shared, and changes a value.
        let mut reopened = table.reopen();
        assert_eq!(reopened.take(UnitType::new(2)), Some('c'));
        reopened.set(UnitType::new(2), 'd');
        let table = reopened.build();
        assert_eq!(table.values().collect::<Vec<_>>(), [&'a', &'d']);
        // A clone reads the same values from the same storage.
        let kept = table.clone();
        assert!(std::ptr::eq(
            kept.get(UnitType::new(0)).unwrap(),
            table.get(UnitType::new(0)).unwrap()
        ));
    }
}
