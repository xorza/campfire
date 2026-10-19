use crate::network_target::NetworkTarget;
use bevy_ecs::entity::{Entity, EntityHashSet};
use bevy_platform::collections::HashMap;
use campfire_net_core::PeerId;
use smallvec::SmallVec;

/// Reusable storage for resolving a [`NetworkTarget`] to connected entities.
///
/// `targets` is the final set returned to the caller. `requested` is scratch storage used to map
/// the peer IDs in a larger [`NetworkTarget::Only`] to entities before intersecting them with the
/// server's connected-client list. Keeping both sets here lets repeated sends reuse their capacity.
#[derive(Debug, Default)]
pub struct NetworkTargetResolver {
    /// Final connected recipients for the current resolution.
    targets: EntityHashSet,
    /// Mapped `Only` recipients before filtering out entities not connected to this server.
    requested: EntityHashSet,
}

impl NetworkTargetResolver {
    /// The count of clients and of requested peers up to which an `Only` target is matched by a
    /// scan of an inline list, past which it is hashed.
    pub(crate) const INLINE: usize = 4;

    /// Resolves `target` against the connected `clients` without allocating after the sets have
    /// grown to the required capacity.
    pub fn resolve(
        &mut self,
        target: &NetworkTarget,
        clients: &[Entity],
        mapping: &HashMap<PeerId, Entity>,
    ) -> &EntityHashSet {
        // Clearing preserves the allocated tables, so steady-state sends only rewrite their
        // contents. Both sets must be cleared because a resolver is reused across target shapes.
        self.targets.clear();
        self.requested.clear();

        match target {
            NetworkTarget::All => self.targets.extend(clients.iter().copied()),
            NetworkTarget::AllExcept(peer_ids) => {
                // Starting from all connected clients and removing exclusions writes the final
                // answer directly, so this branch does not need the `requested` scratch set.
                self.targets.extend(clients.iter().copied());
                for peer_id in peer_ids {
                    if let Some(entity) = mapping.get(peer_id) {
                        self.targets.remove(entity);
                    }
                }
            }
            NetworkTarget::Only(peer_ids) => {
                // For the common small-server case, mapping into an inline list and scanning it is
                // cheaper than hashing and cannot allocate.
                if clients.len() <= Self::INLINE && peer_ids.len() <= Self::INLINE {
                    let requested = peer_ids
                        .iter()
                        .filter_map(|peer_id| mapping.get(peer_id).copied())
                        .collect::<SmallVec<[Entity; Self::INLINE]>>();
                    self.targets.extend(
                        clients
                            .iter()
                            .filter(|entity| requested.contains(entity))
                            .copied(),
                    );
                    return &self.targets;
                }
                // For larger lists, hash the mapped request once and scan connected clients once.
                // This avoids the O(clients * requested) nested membership scan.
                self.requested
                    .extend(peer_ids.iter().filter_map(|peer_id| mapping.get(peer_id)));
                self.targets.extend(
                    clients
                        .iter()
                        .filter(|entity| self.requested.contains(*entity))
                        .copied(),
                );
            }
            NetworkTarget::None => {}
        }

        &self.targets
    }
}
