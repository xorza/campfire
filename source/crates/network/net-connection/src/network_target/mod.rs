use bevy_reflect::Reflect;
use campfire_net_core::PeerId;

use crate::peer_set::PeerSet;

/// The peers a message or an entity is sent to, each set in one form: a list is a sorted
/// [`PeerSet`], and an empty one is [`Self::None`] or [`Self::All`].
#[derive(Debug, Default, Clone, PartialEq, Eq, Reflect)]
pub enum NetworkTarget {
    /// No peer.
    #[default]
    None,
    /// Every peer.
    All,
    /// These peers alone.
    Only(PeerSet),
    /// Every peer except these.
    AllExcept(PeerSet),
}

impl NetworkTarget {
    /// The peer `peer` alone.
    pub fn single(peer: PeerId) -> Self {
        Self::Only(PeerSet::from([peer]))
    }

    /// Every peer except `peers`: [`Self::All`] when it holds none.
    pub fn all_except(peers: impl IntoIterator<Item = PeerId>) -> Self {
        let peers: PeerSet = peers.into_iter().collect();
        if peers.is_empty() {
            return Self::All;
        }
        Self::AllExcept(peers)
    }

    /// Whether `peer` is a target.
    pub fn targets(&self, peer: &PeerId) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Only(peers) => peers.contains(peer),
            Self::AllExcept(peers) => !peers.contains(peer),
        }
    }
}

/// The peers of `iter` alone: [`NetworkTarget::None`] when it holds none.
impl FromIterator<PeerId> for NetworkTarget {
    fn from_iter<I: IntoIterator<Item = PeerId>>(iter: I) -> Self {
        let peers: PeerSet = iter.into_iter().collect();
        if peers.is_empty() {
            return Self::None;
        }
        Self::Only(peers)
    }
}

impl<const N: usize> From<[PeerId; N]> for NetworkTarget {
    fn from(peers: [PeerId; N]) -> Self {
        peers.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
