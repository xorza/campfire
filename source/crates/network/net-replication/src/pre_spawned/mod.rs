use crate::component_type_id::ComponentTypeId;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::component::Components;
use campfire_common::{DomainHasher, HashDomain, HashOutput};
use campfire_net_core::NetId;
use campfire_net_core::NetTick;

use crate::component_registry::ComponentRegistry;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use bevy_reflect::{Reflect, prelude::ReflectDefault};
use campfire_net_timeline::LocalTimeline;
use smallvec::SmallVec;
use tracing::debug;

#[derive(Default, Debug, Copy, Clone, Reflect)]
/// Added to indicate the client has pre-spawned the predicted version of this entity.
///
/// The server should spawn a similar component and replicate it to the client, when the
/// client receive that replicated entity, it will try to match it with the pre-spawned entity
/// using the hash value.
///
/// Pre-spawned entities must be spawned in the `FixedMain` schedule, where the tick is the one
/// the client and the server agree on: a default hash, and any pre-spawn on a receiver, panic
/// outside it. A sender's pre-spawn with a hash of its own may be spawned anywhere.
#[derive(Component)]
#[component(on_add = PreSpawned::on_add)]
#[reflect(Component, Default)]
pub struct PreSpawned {
    /// The hash that identifies the spawned entity, or how the hook makes it.
    hash: PreSpawnHash,

    /// The client link that receives this entity's signature mapping, if one is set.
    ///
    /// This is primarily sender-side configuration. It scopes only Replicon's
    /// signature mapping; replication visibility is still controlled separately.
    pub link: Option<Entity>,
}

/// How a pre-spawned entity's hash is made: by the hook from the entity's archetype and spawn
/// tick, with or without a salt, or given.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
enum PreSpawnHash {
    #[default]
    Default,
    /// The default hash, with a salt the hasher takes after the components: the client id, for
    /// one, tells apart bullets two players spawn at one tick.
    Salted(u64),
    /// The caller's hash, the same on the client and the server, or the one the hook made.
    Given(u64),
}

impl PreSpawned {
    /// You specify the hash yourself, default hasher not used.
    pub const fn new(hash: u64) -> Self {
        Self {
            hash: PreSpawnHash::Given(hash),
            link: None,
        }
    }
    /// Uses default hasher with additional `salt`.
    pub const fn default_with_salt(salt: u64) -> Self {
        Self {
            hash: PreSpawnHash::Salted(salt),
            link: None,
        }
    }

    /// The hash, once the caller gave it or the hook made it.
    pub const fn hash(&self) -> Option<u64> {
        match self.hash {
            PreSpawnHash::Given(hash) => Some(hash),
            PreSpawnHash::Default | PreSpawnHash::Salted(_) => None,
        }
    }

    /// Associates the signature mapping with one client.
    ///
    /// The `link` must be the sender-side client link entity known to Replicon.
    /// Other clients can still receive the entity according to its replication
    /// visibility, but they won't receive this pre-spawn mapping.
    #[must_use]
    pub const fn for_link(mut self, link: Entity) -> Self {
        self.link = Some(link);
        self
    }
}

impl PreSpawned {
    /// Hashes the entity, unless its `PreSpawned` holds a hash, from the components inserted
    /// before or with it and the tick `FixedMain` simulates (11-net-stack/replication.md, A
    /// pre-spawn's tick is the simulated one, and the receiver makes its pre-spawns in
    /// `FixedMain`).
    fn on_add(mut deferred_world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let salt = match deferred_world.entity(entity).get::<Self>().unwrap().hash {
            PreSpawnHash::Given(_) => return,
            PreSpawnHash::Default => None,
            PreSpawnHash::Salted(salt) => Some(salt),
        };

        // The hash holds the tick the entity is spawned at, which only the simulation of a tick
        // gives the same on the client and the server.
        let tick = deferred_world.resource::<LocalTimeline>().simulated_tick();
        assert!(
            tick.is_some(),
            "{entity:?} gets a default PreSpawned hash outside FixedMain, where no tick is \
             simulated: spawn it in FixedMain, or give it a hash with PreSpawned::new"
        );
        let tick = tick.unwrap();
        let components = deferred_world.components();
        let component_registry = deferred_world.resource::<ComponentRegistry>();
        let entity_ref = deferred_world.entity(entity);
        let hash = compute_default_hash(
            component_registry,
            components,
            entity_ref.archetype(),
            tick,
            salt,
        );
        // update component with the computed hash
        debug!(
            ?entity,
            ?tick,
            hash = ?hash,
            "PreSpawned hook, setting the hash on the component"
        );
        deferred_world
            .entity_mut(entity)
            .get_mut::<Self>()
            .unwrap()
            .hash = PreSpawnHash::Given(hash);
    }
}

/// The tag of the default pre-spawn hash.
const DEFAULT_HASH_DOMAIN: HashDomain<DefaultHash> = HashDomain::new("campfire/pre-spawn/v1");

/// A default pre-spawn hash: the first 8 bytes of its domain hash, little-endian, as
/// `PreSpawned` holds a `u64`.
#[derive(Debug)]
struct DefaultHash(u64);

impl HashOutput for DefaultHash {
    fn of_hash(bytes: [u8; 32]) -> Self {
        let [b0, b1, b2, b3, b4, b5, b6, b7, ..] = bytes;
        Self(u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]))
    }
}

/// The default hash that matches a server's entity with the client's pre-spawned one: the domain
/// hash of the spawn tick, the count and the sorted net ids of the entity's registered components,
/// and the salt when one is given, each in explicit little-endian bytes, so a client and a server
/// in two processes agree (11-net-stack/api.md, The registry hash is a domain hash).
pub(crate) fn compute_default_hash(
    component_registry: &ComponentRegistry,
    components: &Components,
    archetype: &Archetype,
    tick: NetTick,
    salt: Option<u64>,
) -> u64 {
    let mut ids = archetype
        .iter_components()
        .filter_map(|component_id| {
            let type_id = components.get_info(component_id)?.type_id()?;
            component_registry.net_id(&ComponentTypeId::from(type_id))
        })
        .collect::<SmallVec<[NetId; 16]>>();
    // An archetype lists its components in an order of its own.
    ids.sort_unstable();
    let count = u32::try_from(ids.len()).expect("a registry's net ids are u16");
    let mut hasher = DomainHasher::new(DEFAULT_HASH_DOMAIN);
    hasher
        .update(&tick.0.to_le_bytes())
        .update(&count.to_le_bytes());
    for id in ids {
        hasher.update(&id.get().to_le_bytes());
    }
    match salt {
        None => hasher.update(&[0]),
        Some(salt) => hasher.update(&[1]).update(&salt.to_le_bytes()),
    };
    hasher.finish().0
}

#[cfg(test)]
mod tests;
