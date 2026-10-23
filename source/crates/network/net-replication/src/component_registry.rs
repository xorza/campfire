use crate::component_type_id::ComponentTypeId;
use bevy_ecs::prelude::*;
use bevy_reflect::TypePath;

use crate::replication_kind::ReplicationKind;
use campfire_common::HashDomain;
use campfire_net_core::RegistrySeal;
use campfire_net_core::{NetId, TypeMapper};
use campfire_net_core::{RegistryHash, RegistryHasher};

/// The tag of the component registry's hash.
const HASH_DOMAIN: HashDomain<RegistryHash> = HashDomain::new("campfire/component-registry/v1");

/// The replicated components, each with its network id and how it replicates; a component that
/// replicates through Replicon's own API is unknown to prediction and interpolation.
#[derive(Debug, Default, Clone, Resource, TypePath)]
pub struct ComponentRegistry {
    type_map: TypeMapper<ComponentTypeId>,
    seal: RegistrySeal,
    /// How each component replicates, as its registration set it, at the index of its id.
    replication: Vec<Option<ReplicationKind>>,
}

impl ComponentRegistry {
    pub fn is_registered<C: 'static>(&self) -> bool {
        self.contains(&ComponentTypeId::of::<C>())
    }

    pub fn contains(&self, type_id: &ComponentTypeId) -> bool {
        self.type_map.net_id(type_id).is_some()
    }

    /// The network id of the component of `type_id`, if it is registered.
    pub(crate) fn net_id(&self, type_id: &ComponentTypeId) -> Option<NetId> {
        self.type_map.net_id(type_id).copied()
    }

    pub fn register_component<C: Component>(&mut self) {
        self.open();
        self.type_map.add::<C>();
    }

    fn open(&self) {
        self.seal.assert_open("component registry");
    }

    /// Seals the registry, as the shared replication plugin's `finish` does. Its hash holds each
    /// component's type and how it replicates, in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::new(HASH_DOMAIN);
        for (index, (_, name)) in self.type_map.iter().enumerate() {
            hasher.name(name);
            hasher.write(&[match self.replication.get(index).copied().flatten() {
                None => 0,
                Some(ReplicationKind::Full) => 1,
                Some(ReplicationKind::Diff) => 2,
            }]);
        }
        self.seal.seal("component registry", hasher.finish());
    }

    /// The hash peers compare, once the registry is sealed.
    pub const fn hash(&self) -> Option<RegistryHash> {
        self.seal.hash()
    }

    /// How the component replicates, if it does.
    pub fn replication(&self, type_id: &ComponentTypeId) -> Option<ReplicationKind> {
        let net_id = self.type_map.net_id(type_id)?;
        self.replication
            .get(usize::from(net_id.get()))
            .copied()
            .flatten()
    }

    /// Records how `C` replicates.
    ///
    /// # Panics
    ///
    /// When `C` was registered to replicate the other way: one component replicates one way.
    pub(crate) fn set_replication<C: Component>(&mut self, replication: ReplicationKind) {
        self.open();
        let index = usize::from(
            self.type_map
                .net_id(&ComponentTypeId::of::<C>())
                .expect("a component registers before how it replicates")
                .get(),
        );
        if index >= self.replication.len() {
            self.replication.resize(index + 1, None);
        }
        let previous = self.replication[index].replace(replication);
        assert!(
            previous.is_none_or(|previous| previous == replication),
            "{} replicates both whole and as diffs",
            core::any::type_name::<C>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    struct Health;

    fn hash_of(build: impl FnOnce(&mut ComponentRegistry)) -> RegistryHash {
        let mut registry = ComponentRegistry::default();
        build(&mut registry);
        registry.seal();
        registry.hash().unwrap()
    }

    /// A component and how it replicates each change the hash, so the protocol check tells
    /// registries apart; the same registrations agree.
    #[test]
    fn each_component_and_its_replication_enter_the_hash() {
        let empty = hash_of(|_| {});
        let plain = hash_of(ComponentRegistry::register_component::<Health>);
        let full = hash_of(|registry| {
            registry.register_component::<Health>();
            registry.set_replication::<Health>(ReplicationKind::Full);
        });
        let diff = hash_of(|registry| {
            registry.register_component::<Health>();
            registry.set_replication::<Health>(ReplicationKind::Diff);
        });
        let hashes = [empty, plain, full, diff];
        for (i, hash) in hashes.iter().enumerate() {
            for other in &hashes[i + 1..] {
                assert_ne!(hash, other);
            }
        }
        assert_eq!(
            hash_of(ComponentRegistry::register_component::<Health>),
            plain
        );
    }

    #[test]
    #[should_panic(
        expected = "the component registry takes a registration after its plugin finished"
    )]
    fn a_sealed_registry_refuses_a_component() {
        let mut registry = ComponentRegistry::default();
        registry.seal();
        registry.register_component::<Health>();
    }
}
