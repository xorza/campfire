use campfire_sim::StableId;

use crate::body_index::IndexedBody;

/// Two bodies within each other's personal space, by stable id, the first the lower, which orders
/// the contacts a stage pushes and resolves: two walkers, or a walker and a static body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Contact {
    pub(crate) first: StableId,
    pub(crate) second: StableId,
    pub(crate) bodies: ContactBodies,
}

/// The bodies of a contact: two walkers, by their indices among the colliders, the first the
/// lower; or a walker, by its index, and a static body, as the static index holds it, which no
/// contact moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContactBodies {
    Walkers { first: usize, second: usize },
    Static { walker: usize, body: IndexedBody },
}

impl Contact {
    /// The indices of the walkers among the colliders that it may push.
    pub(crate) fn walkers(&self) -> impl Iterator<Item = usize> {
        let (first, second) = match self.bodies {
            ContactBodies::Walkers { first, second } => (first, Some(second)),
            ContactBodies::Static { walker, .. } => (walker, None),
        };
        [first].into_iter().chain(second)
    }
}
