use std::iter::Enumerate;
use std::marker::PhantomData;
use std::slice;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_names::MatchNames;
use campfire_values::NameId;

use crate::book::Book;
use crate::per_id::PerId;

/// A value for some of the match's ids of `K`, by id: a capability's package data about each unit
/// type that has it, such as its AI or a part its units spawn with, or what a kind's capability
/// resolves of each action of its kind (04-capabilities/actions.md, Each kind owns its section). A
/// clone shares the values, so a reader keeps the book itself, not a copy of what it holds;
/// `ByIdBuilder` builds it.
#[derive(Resource, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct ById<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> {
    entries: Arc<[Option<T>]>,
    ids: PhantomData<fn() -> K>,
}

/// Each id of a `ById` that has a value, with it, by id.
#[derive(Debug, Clone)]
pub struct ByIdIter<'a, K, T> {
    entries: Enumerate<slice::Iter<'a, Option<T>>>,
    ids: PhantomData<fn() -> K>,
}

/// A `ById` as it is built: each id's value set once, in any order.
#[derive(Debug)]
pub struct ByIdBuilder<K: NameId, T> {
    entries: Vec<Option<T>>,
    ids: PhantomData<fn() -> K>,
}

impl<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> ById<K, T> {
    /// The value of `id`; none for an id of none.
    pub fn get(&self, id: K) -> Option<&T> {
        self.entries.get(id.index())?.as_ref()
    }

    /// The values of the ids that have one, by id.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().flatten()
    }

    /// Each id that has a value, with it, by id.
    pub fn iter(&self) -> ByIdIter<'_, K, T> {
        ByIdIter {
            entries: self.entries.iter().enumerate(),
            ids: PhantomData,
        }
    }

    /// Whether no id has a value.
    pub fn is_empty(&self) -> bool {
        self.values().next().is_none()
    }

    /// The book as it is built again, its values moved out, as an extension of the book changes
    /// it before the book is shared.
    pub fn reopen(mut self) -> ByIdBuilder<K, T> {
        let entries = Arc::get_mut(&mut self.entries).expect("a book changes before it is shared");
        ByIdBuilder {
            entries: entries.iter_mut().map(Option::take).collect(),
            ids: PhantomData,
        }
    }
}

impl<'a, K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> IntoIterator
    for &'a ById<K, T>
{
    type Item = (K, &'a T);
    type IntoIter = ByIdIter<'a, K, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K: NameId, T> Iterator for ByIdIter<'a, K, T> {
    type Item = (K, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.entries
            .find_map(|(at, value)| Some((K::at(at), value.as_ref()?)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.entries.size_hint().1)
    }
}

impl<K: NameId, T> ByIdBuilder<K, T> {
    /// Gives `id` its `value`, which it has none of yet.
    pub fn set(&mut self, id: K, value: T) {
        let index = id.index();
        if self.entries.len() <= index {
            self.entries.resize_with(index + 1, || None);
        }
        assert!(self.entries[index].is_none(), "an id has one value");
        self.entries[index] = Some(value);
    }

    /// Takes `id`'s value out.
    pub fn take(&mut self, id: K) -> Option<T> {
        self.entries.get_mut(id.index())?.take()
    }

    /// Gives `id` `value`, when it is some, as `set` does.
    pub fn set_some(&mut self, id: K, value: Option<T>) {
        if let Some(value) = value {
            self.set(id, value);
        }
    }
}

impl<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> ByIdBuilder<K, T> {
    /// The book of the values set.
    pub fn build(self) -> ById<K, T> {
        ById {
            entries: Arc::from(self.entries),
            ids: PhantomData,
        }
    }
}

impl<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> Clone for ById<K, T> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
            ids: PhantomData,
        }
    }
}

impl<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> Default for ById<K, T> {
    fn default() -> Self {
        Self {
            entries: Arc::new([]),
            ids: PhantomData,
        }
    }
}

impl<K: NameId, T> Default for ByIdBuilder<K, T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            ids: PhantomData,
        }
    }
}

impl<K: NameId + Send + Sync + 'static, T: PerId<K>> Book for ById<K, T> {
    const SCRIPTS_ONLY: bool = T::SCRIPTS_ONLY;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::mem;

    use campfire_values::NameId;

    use crate::by_id::ById;

    impl<K: NameId + Send + Sync + 'static, T: Send + Sync + 'static> ById<K, T> {
        /// Gives `id` its `value` in a built book, which no clone shares yet, as a test gives an
        /// id it adds its value: the book is built again.
        pub fn set(&mut self, id: K, value: T) {
            let mut built = mem::take(self).reopen();
            built.set(id, value);
            *self = built.build();
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::UnitType;

    use super::*;

    #[test]
    fn an_id_has_its_value_or_none() {
        let mut built = ByIdBuilder::default();
        built.set(UnitType::new(2), 'c');
        built.set(UnitType::new(0), 'a');
        // None gives an id no value, within the table or past its end.
        built.set_some(UnitType::new(1), None);
        built.set_some(UnitType::new(4), None);
        let table: ById<UnitType, char> = built.build();
        let read = [0, 1, 2, 3, 4].map(|index| table.get(UnitType::new(index)).copied());
        assert_eq!(read, [Some('a'), None, Some('c'), None, None]);
        assert_eq!(table.values().collect::<Vec<_>>(), [&'a', &'c']);
        let held: Vec<_> = table.iter().collect();
        assert_eq!(held, [(UnitType::new(0), &'a'), (UnitType::new(2), &'c')]);
        // A loop over the book reads what `iter` does; its three entries, to the last value, as a
        // none past the end adds none, bound the two it finds.
        assert_eq!((&table).into_iter().collect::<Vec<_>>(), held);
        assert_eq!(table.iter().size_hint(), (0, Some(3)));
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
