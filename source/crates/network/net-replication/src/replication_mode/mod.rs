use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::RemoteId;
use smallvec::SmallVec;

/// Controls which peers an entity is replicated to.
///
/// Each `ReplicationTarget` stores a `ReplicationMode` that determines
/// the set of link entities (connections) through which the entity will
/// be sent. Most users should use the convenience constructors on
/// `Replicate` rather than constructing a mode directly.
#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
pub enum ReplicationMode {
    /// Replicates to the links whose peer the [`NetworkTarget`] names.
    Target(NetworkTarget),
    /// Replicates to these link entities.
    Links(SmallVec<[Entity; 2]>),
}

/// Replicates to every link.
impl Default for ReplicationMode {
    fn default() -> Self {
        Self::Target(NetworkTarget::All)
    }
}

impl ReplicationMode {
    /// Pure visibility predicate behind the
    /// [`VisibilityFilter`](bevy_replicon::prelude::VisibilityFilter) impls on `ReplicationTarget`.
    ///
    /// `link` is the link entity being evaluated and `remote` its [`RemoteId`]
    /// (absent if the link has no id yet).
    pub(crate) fn is_visible_for(&self, link: Entity, remote: Option<&RemoteId>) -> bool {
        // Fail closed: links without a `RemoteId` receive nothing. Inserting
        // `RemoteId` re-evaluates (via Replicon's client-insert observer, or its
        // new-client evaluation when `RemoteId` was already present), so the
        // hidden window is transient.
        let Some(remote) = remote else {
            return false;
        };
        match self {
            Self::Target(target) => target.targets(&remote.0),
            Self::Links(links) => links.contains(&link),
        }
    }
}

#[cfg(test)]
mod tests;
