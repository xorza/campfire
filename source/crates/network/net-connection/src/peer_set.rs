use bevy_reflect::Reflect;
use campfire_net_core::PeerId;
use core::ops::Deref;
use std::sync::Arc;

/// A set of peers: sorted, each once, and shared, so a clone of a target that holds it is a count
/// and not a copy of the list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
#[reflect(opaque)]
#[reflect(Clone, Debug, PartialEq, Hash)]
pub struct PeerSet(Arc<[PeerId]>);

impl PeerSet {
    /// Whether `peer` is in the set.
    pub fn contains(&self, peer: &PeerId) -> bool {
        self.0.binary_search(peer).is_ok()
    }
}

/// The peers of `iter`, sorted and each once.
impl FromIterator<PeerId> for PeerSet {
    fn from_iter<I: IntoIterator<Item = PeerId>>(iter: I) -> Self {
        let mut peers: Vec<PeerId> = iter.into_iter().collect();
        peers.sort_unstable();
        peers.dedup();
        Self(peers.into())
    }
}

impl<const N: usize> From<[PeerId; N]> for PeerSet {
    fn from(peers: [PeerId; N]) -> Self {
        peers.into_iter().collect()
    }
}

/// The peers, in order.
impl Deref for PeerSet {
    type Target = [PeerId];

    fn deref(&self) -> &[PeerId] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a PeerSet {
    type Item = &'a PeerId;
    type IntoIter = core::slice::Iter<'a, PeerId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
