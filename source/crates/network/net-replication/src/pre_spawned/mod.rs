use crate::component_kind::ComponentKind;
use bevy_ecs::archetype::Archetype;
use bevy_ecs::component::Components;
use campfire_net_core::NetTick;
use campfire_net_core::{NetId, StableHasher};
use core::hash::{Hash, Hasher};

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
///
/// ```rust
/// # use campfire_net_replication::{AppComponentExt, PreSpawned, VisibilityExt};
/// // Default hashing implementation: (tick + components)
/// PreSpawned::default();
///
/// // Default hashing implementation with additional user-provided salt:
/// let client_id: u64 = 12345;
/// PreSpawned::default_with_salt(client_id);
///
/// // User-provided custom hash
/// let custom_hash: u64 = 1;
/// PreSpawned::new(1);
/// ```
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

/// Hook calculates the hash (if missing), and updates the PreSpawned component.
/// Since this is a hook, it will calculate based on components inserted before or alongside the
/// PreSpawned component, on the same tick that PreSpawned was inserted.
impl PreSpawned {
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

/// Compute the default PreSpawned hash used to match server entities with pre-spawned client
/// entities
pub(crate) fn compute_default_hash(
    component_registry: &ComponentRegistry,
    components: &Components,
    archetype: &Archetype,
    tick: NetTick,
    salt: Option<u64>,
) -> u64 {
    // A seeded std or Bevy hasher differs between processes, which the client and the server are.
    let mut hasher = StableHasher::default();

    // we include the spawn tick in the hash
    tick.hash(&mut hasher);

    // NOTE: we cannot call hash() multiple times because the components in the archetype
    //  might get iterated in any order!
    //  Instead we will get the sorted list of types to hash first, sorted by type_id
    let mut kinds_to_hash = archetype
        .iter_components()
        .filter_map(|component_id| {
            let type_id = components.get_info(component_id)?.type_id()?;
            component_registry
                .kind_map
                .net_id(&ComponentKind::from(type_id))
                .copied()
        })
        .collect::<SmallVec<[NetId; 16]>>();
    kinds_to_hash.sort_unstable();
    for kind in kinds_to_hash {
        kind.hash(&mut hasher);
    }

    // if a user salt is provided, hash after the sorted component list
    if let Some(salt) = salt {
        salt.hash(&mut hasher);
    }

    hasher.finish()
}

#[cfg(test)]
mod tests;
