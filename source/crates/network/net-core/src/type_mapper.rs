use crate::net_id::NetId;
use bevy_platform::collections::HashMap;
use core::any::TypeId;
use core::hash::Hash;

pub trait RegisteredType: From<TypeId> + Copy + PartialEq + Eq + Hash {}

/// The types of a registry, each with the id it travels as: the ids are dense, `0..len` in
/// registration order, so an id indexes its type.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeMapper<K: RegisteredType> {
    /// Each type and its name, at the index of its id.
    type_ids: Vec<(K, &'static str)>,
    ids: HashMap<K, NetId>,
}

impl<K: RegisteredType> Default for TypeMapper<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: RegisteredType> TypeMapper<K> {
    pub fn new() -> Self {
        Self {
            type_ids: Vec::new(),
            ids: HashMap::default(),
        }
    }

    /// Register a new type, with the next id in registration order.
    ///
    /// # Panics
    ///
    /// Past 65,536 types: a registry's ids are `u16`.
    pub fn add<T: 'static>(&mut self) -> K {
        let type_id = K::from(TypeId::of::<T>());
        if self.ids.contains_key(&type_id) {
            return type_id;
        }
        let net_id = NetId::new(
            u16::try_from(self.type_ids.len()).expect("a registry holds at most 65,536 types"),
        );
        self.type_ids.push((type_id, core::any::type_name::<T>()));
        self.ids.insert(type_id, net_id);
        type_id
    }

    pub fn type_id(&self, net_id: NetId) -> Option<&K> {
        self.type_ids
            .get(usize::from(net_id.get()))
            .map(|(type_id, _)| type_id)
    }

    pub fn net_id(&self, type_id: &K) -> Option<&NetId> {
        self.ids.get(type_id)
    }

    /// The name of the type `net_id` names.
    pub fn name_at(&self, net_id: NetId) -> Option<&'static str> {
        self.type_ids
            .get(usize::from(net_id.get()))
            .map(|(_, name)| *name)
    }

    pub fn name(&self, type_id: &K) -> Option<&'static str> {
        self.ids
            .get(type_id)
            .map(|net_id| self.type_ids[usize::from(net_id.get())].1)
    }

    pub const fn len(&self) -> usize {
        self.type_ids.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.type_ids.is_empty()
    }

    /// Each type and its name, in the order of their ids.
    pub fn iter(&self) -> core::slice::Iter<'_, (K, &'static str)> {
        self.type_ids.iter()
    }
}

impl<'a, K: RegisteredType> IntoIterator for &'a TypeMapper<K> {
    type Item = &'a (K, &'static str);
    type IntoIter = core::slice::Iter<'a, (K, &'static str)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    struct TestTypeId(TypeId);

    impl From<TypeId> for TestTypeId {
        fn from(type_id: TypeId) -> Self {
            Self(type_id)
        }
    }

    impl RegisteredType for TestTypeId {}

    /// Types get the ids 0 and 1 in registration order, and a type registered again keeps its
    /// id; an id beyond them names no type.
    #[test]
    fn the_ids_are_dense_in_the_order_of_registration() {
        let mut mapper = TypeMapper::<TestTypeId>::new();
        let first = mapper.add::<u8>();
        let second = mapper.add::<u16>();
        assert_eq!(mapper.add::<u8>(), first);

        assert_eq!(mapper.net_id(&first), Some(&NetId::new(0)));
        assert_eq!(mapper.net_id(&second), Some(&NetId::new(1)));
        assert_eq!(mapper.type_id(NetId::new(1)), Some(&second));
        assert_eq!(mapper.type_id(NetId::new(2)), None);
        assert_eq!(mapper.name(&second), Some("u16"));
        assert_eq!(mapper.name_at(NetId::new(1)), Some("u16"));
        assert_eq!(mapper.name_at(NetId::new(2)), None);
        assert_eq!(
            mapper
                .iter()
                .map(|&(type_id, _)| type_id)
                .collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(mapper.len(), 2);
    }
}
