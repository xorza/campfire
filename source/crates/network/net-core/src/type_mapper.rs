use crate::net_id::NetId;
use bevy_platform::collections::HashMap;
use core::any::TypeId;
use core::hash::Hash;

pub trait TypeKind: From<TypeId> + Copy + PartialEq + Eq + Hash {}

/// The types of a registry, each with the id it travels as: the ids are dense, `0..len` in
/// registration order, so an id indexes its type.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeMapper<K: TypeKind> {
    /// Each type and its name, at the index of its id.
    kinds: Vec<(K, &'static str)>,
    ids: HashMap<K, NetId>,
}

impl<K: TypeKind> Default for TypeMapper<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: TypeKind> TypeMapper<K> {
    pub fn new() -> Self {
        Self {
            kinds: Vec::new(),
            ids: HashMap::default(),
        }
    }

    /// Register a new type, with the next id in registration order.
    ///
    /// # Panics
    ///
    /// Past 65,536 types: a registry's ids are `u16`.
    pub fn add<T: 'static>(&mut self) -> K {
        let kind = K::from(TypeId::of::<T>());
        if self.ids.contains_key(&kind) {
            return kind;
        }
        let net_id = NetId::new(
            u16::try_from(self.kinds.len()).expect("a registry holds at most 65,536 types"),
        );
        self.kinds.push((kind, core::any::type_name::<T>()));
        self.ids.insert(kind, net_id);
        kind
    }

    pub fn kind(&self, net_id: NetId) -> Option<&K> {
        self.kinds
            .get(usize::from(net_id.get()))
            .map(|(kind, _)| kind)
    }

    pub fn net_id(&self, kind: &K) -> Option<&NetId> {
        self.ids.get(kind)
    }

    /// The name of the type `net_id` names.
    pub fn name_at(&self, net_id: NetId) -> Option<&'static str> {
        self.kinds
            .get(usize::from(net_id.get()))
            .map(|(_, name)| *name)
    }

    pub fn name(&self, kind: &K) -> Option<&'static str> {
        self.ids
            .get(kind)
            .map(|net_id| self.kinds[usize::from(net_id.get())].1)
    }

    pub const fn len(&self) -> usize {
        self.kinds.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Each type and its name, in the order of their ids.
    pub fn iter(&self) -> core::slice::Iter<'_, (K, &'static str)> {
        self.kinds.iter()
    }
}

impl<'a, K: TypeKind> IntoIterator for &'a TypeMapper<K> {
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
    struct Kind(TypeId);

    impl From<TypeId> for Kind {
        fn from(type_id: TypeId) -> Self {
            Self(type_id)
        }
    }

    impl TypeKind for Kind {}

    /// Types get the ids 0 and 1 in registration order, and a type registered again keeps its
    /// id; an id beyond them names no type.
    #[test]
    fn ids_are_dense_in_registration_order() {
        let mut mapper = TypeMapper::<Kind>::new();
        let first = mapper.add::<u8>();
        let second = mapper.add::<u16>();
        assert_eq!(mapper.add::<u8>(), first);

        assert_eq!(mapper.net_id(&first), Some(&NetId::new(0)));
        assert_eq!(mapper.net_id(&second), Some(&NetId::new(1)));
        assert_eq!(mapper.kind(NetId::new(1)), Some(&second));
        assert_eq!(mapper.kind(NetId::new(2)), None);
        assert_eq!(mapper.name(&second), Some("u16"));
        assert_eq!(mapper.name_at(NetId::new(1)), Some("u16"));
        assert_eq!(mapper.name_at(NetId::new(2)), None);
        assert_eq!(
            mapper.iter().map(|&(kind, _)| kind).collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(mapper.len(), 2);
    }
}
