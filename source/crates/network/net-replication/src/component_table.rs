use core::iter::Map;
use core::slice;

use crate::component_type_id::ComponentTypeId;

/// What a registry holds of each component it registers, each once, sorted by its type, so a
/// lookup is a binary search. The order is the type ids', which one build holds and another need
/// not, so nothing a peer compares reads it.
#[derive(Debug)]
pub struct ComponentTable<E>(Vec<(ComponentTypeId, E)>);

impl<E> Default for ComponentTable<E> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<E> ComponentTable<E> {
    /// The entry of `type_id`; none for a component the table holds none of.
    pub fn get(&self, type_id: ComponentTypeId) -> Option<&E> {
        let at = self.at(type_id).ok()?;
        Some(&self.0[at].1)
    }

    /// The entry of `type_id` to change; none for a component the table holds none of.
    pub fn get_mut(&mut self, type_id: ComponentTypeId) -> Option<&mut E> {
        let at = self.at(type_id).ok()?;
        Some(&mut self.0[at].1)
    }

    /// The entry of `type_id`, which `new` makes first when the table holds none.
    pub fn entry(&mut self, type_id: ComponentTypeId, new: impl FnOnce() -> E) -> &mut E {
        let at = self.at(type_id).unwrap_or_else(|at| {
            self.0.insert(at, (type_id, new()));
            at
        });
        &mut self.0[at].1
    }

    /// Each component's entry, in the order of their types.
    pub fn iter(&self) -> ComponentEntries<'_, E> {
        let split: fn(&(ComponentTypeId, E)) -> (ComponentTypeId, &E) =
            |(type_id, entry)| (*type_id, entry);
        self.0.iter().map(split)
    }

    fn at(&self, type_id: ComponentTypeId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&type_id, |&(held, _)| held)
    }
}

/// The entries of a `ComponentTable`, in the order of their types.
pub type ComponentEntries<'a, E> = Map<
    slice::Iter<'a, (ComponentTypeId, E)>,
    fn(&'a (ComponentTypeId, E)) -> (ComponentTypeId, &'a E),
>;

impl<'a, E> IntoIterator for &'a ComponentTable<E> {
    type Item = (ComponentTypeId, &'a E);
    type IntoIter = ComponentEntries<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_goes_in_once_and_is_found_by_its_type() {
        let mut table = ComponentTable::<u8>::default();
        let [a, b] = [ComponentTypeId::of::<u8>(), ComponentTypeId::of::<u16>()];
        assert_eq!(table.get(a), None);
        *table.entry(b, || 1) += 1;
        *table.entry(a, || 5) += 1;
        // The second entry of `b` finds the first, and `new` makes nothing.
        assert_eq!(*table.entry(b, || unreachable!("b is held")), 2);
        assert_eq!((table.get(a), table.get(b)), (Some(&6), Some(&2)));
        *table.get_mut(a).unwrap() = 0;
        // The entries go in the order of their types, whichever went in first.
        let (low, high) = if a < b {
            ((a, 0), (b, 2))
        } else {
            ((b, 2), (a, 0))
        };
        let entries: Vec<(ComponentTypeId, u8)> = table.iter().map(|(id, &e)| (id, e)).collect();
        assert_eq!(entries, [low, high]);
    }
}
