use std::any::{Any, TypeId};
use std::fmt;
use std::slice;

/// A value a list by type holds, seen as `Any`, so the list finds the item of its own type. A
/// trait object whose trait has it as a supertrait gives its item's own `Any` through its table.
pub trait TypedItem: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> TypedItem for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Items of distinct types, in the order added, each found by a search of the list of their
/// types, which reads no item.
pub struct TypedList<T: ?Sized> {
    types: Vec<TypeId>,
    items: Vec<Box<T>>,
}

impl<T: ?Sized + TypedItem> TypedList<T> {
    /// Adds `item`, of a type the list holds none of yet. Its place.
    pub fn add(&mut self, item: Box<T>) -> usize {
        let ty = (*item).as_any().type_id();
        debug_assert!(!self.types.contains(&ty), "one item of each type");
        self.types.push(ty);
        self.items.push(item);
        self.items.len() - 1
    }

    /// The place of the item of type `U`, when one was added.
    pub fn index_of<U: Any>(&self) -> Option<usize> {
        let ty = TypeId::of::<U>();
        self.types.iter().position(|&held| held == ty)
    }

    /// The item of type `U`, when one was added.
    pub fn get<U: Any>(&self) -> Option<&U> {
        let at = self.index_of::<U>()?;
        (*self.items[at]).as_any().downcast_ref()
    }

    /// The item of type `U`, to change, when one was added.
    pub fn get_mut<U: Any>(&mut self) -> Option<&mut U> {
        let at = self.index_of::<U>()?;
        Some(self.typed_mut(at))
    }

    /// The item at `at`, of type `U`.
    pub fn typed_mut<U: Any>(&mut self, at: usize) -> &mut U {
        (*self.items[at])
            .as_any_mut()
            .downcast_mut()
            .expect("the item at a place is of the type found there")
    }

    /// The item at `at`.
    pub fn at(&self, at: usize) -> &T {
        &self.items[at]
    }

    /// The item at `at`, to change.
    pub fn at_mut(&mut self, at: usize) -> &mut T {
        &mut self.items[at]
    }

    /// Every item, in the order added.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> {
        self.items.iter().map(|item| &**item)
    }

    /// Every item, in the order added, to change.
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.items.iter_mut().map(|item| &mut **item)
    }
}

impl<T: ?Sized> Default for TypedList<T> {
    fn default() -> Self {
        Self {
            types: Vec::new(),
            items: Vec::new(),
        }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for TypedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.items).finish()
    }
}

impl<'a, T: ?Sized + TypedItem> IntoIterator for &'a TypedList<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Map<slice::Iter<'a, Box<T>>, fn(&'a Box<T>) -> &'a T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter().map(|item| &**item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait Named: TypedItem + fmt::Debug {
        fn name(&self) -> &'static str;
    }

    #[derive(Debug, PartialEq, Eq)]
    struct First(u8);

    #[derive(Debug, PartialEq, Eq)]
    struct Second(u8);

    impl Named for First {
        fn name(&self) -> &'static str {
            "first"
        }
    }

    impl Named for Second {
        fn name(&self) -> &'static str {
            "second"
        }
    }

    #[test]
    fn each_item_is_found_by_its_own_type_in_the_order_added() {
        let mut list = TypedList::<dyn Named>::default();
        assert_eq!(list.get::<First>(), None);
        assert_eq!(list.add(Box::new(Second(2))), 0);
        assert_eq!(list.add(Box::new(First(1))), 1);
        // The list holds its items' types, not the boxes'.
        assert_eq!(list.index_of::<Second>(), Some(0));
        assert_eq!(list.index_of::<First>(), Some(1));
        assert_eq!(list.index_of::<Box<dyn Named>>(), None);
        assert_eq!(list.get::<First>(), Some(&First(1)));
        list.get_mut::<Second>().unwrap().0 = 4;
        assert_eq!(list.typed_mut::<Second>(0), &Second(4));
        assert_eq!(list.at(1).name(), "first");
        let names: Vec<_> = (&list).into_iter().map(Named::name).collect();
        assert_eq!(names, ["second", "first"]);
        assert_eq!(list.iter_mut().len(), 2);
    }
}
