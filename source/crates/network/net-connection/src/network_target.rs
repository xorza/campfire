use crate::peer_set::PeerSet;
use bevy_reflect::Reflect;
use campfire_net_core::PeerId;

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
mod tests {
    use super::*;

    use crate::network_target_resolver::NetworkTargetResolver;
    use bevy_ecs::entity::{Entity, EntityHashSet};
    use bevy_platform::collections::HashMap;

    fn peer(id: u64) -> PeerId {
        PeerId::Entity(id)
    }

    fn assert_entities(actual: &EntityHashSet, expected: &[Entity]) {
        assert_eq!(actual.len(), expected.len());
        assert!(expected.iter().all(|entity| actual.contains(entity)));
    }

    /// Peer 2 maps to an entity that is not a connected client, and peer 3 to none: each variant
    /// resolves to the connected clients it names.
    #[test]
    fn resolver_reuses_storage_and_filters_to_connected_clients() {
        let clients = [Entity::from_bits(1), Entity::from_bits(2)];
        let mapping = HashMap::from_iter([
            (peer(0), clients[0]),
            (peer(1), clients[1]),
            (peer(2), Entity::from_bits(3)),
        ]);
        let mut resolver = NetworkTargetResolver::default();

        let cases = [
            (NetworkTarget::All, &clients[..]),
            (
                NetworkTarget::from([peer(1), peer(2), peer(3)]),
                &clients[1..],
            ),
            (NetworkTarget::all_except([peer(1), peer(2)]), &clients[..1]),
            (NetworkTarget::None, &[]),
            (NetworkTarget::all_except([peer(0)]), &clients[1..]),
            (NetworkTarget::all_except([peer(3)]), &clients[..]),
            (NetworkTarget::single(peer(1)), &clients[1..]),
            (NetworkTarget::single(peer(2)), &[]),
            (NetworkTarget::single(peer(3)), &[]),
        ];
        for (target, expected) in cases {
            assert_entities(resolver.resolve(&target, &clients, &mapping), expected);
        }
    }

    /// Past the inline capacity, `Only` takes its hashed path, with the same answer: of peers 1, 3,
    /// 5, 6 and 7, the ones that map to a connected client, as 6 maps to no client and 7 to no
    /// entity.
    #[test]
    fn a_large_only_list_resolves_through_the_hashed_path() {
        let count = NetworkTargetResolver::INLINE as u64 + 2;
        let entity = |id| Entity::from_bits(id + 1);
        let clients = (0..count).map(entity).collect::<Vec<_>>();
        let mapping = (0..=count)
            .map(|id| (peer(id), entity(id)))
            .collect::<HashMap<_, _>>();
        let requested = [1, 3, 5, count, count + 1].map(peer);
        let mut resolver = NetworkTargetResolver::default();
        let expected = [1, 3, 5].map(entity);
        assert_entities(
            resolver.resolve(&NetworkTarget::from(requested), &clients, &mapping),
            &expected,
        );
    }

    /// A list keeps each peer once, in order, so the same set has one form: a lone peer and a
    /// list of it are equal, and an empty list is none, or all for an exclusion.
    #[test]
    fn collecting_sorts_drops_duplicates_and_keeps_one_form() {
        assert_eq!(
            core::iter::empty().collect::<NetworkTarget>(),
            NetworkTarget::None
        );
        assert_eq!(NetworkTarget::all_except([]), NetworkTarget::All);
        assert_eq!(
            NetworkTarget::from([peer(0), peer(0)]),
            NetworkTarget::single(peer(0))
        );
        let NetworkTarget::Only(peers) = NetworkTarget::from([peer(1), peer(0), peer(1), peer(2)])
        else {
            panic!("a list of peers is `Only`");
        };
        assert_eq!(*peers, [peer(0), peer(1), peer(2)]);
        assert_eq!(
            NetworkTarget::all_except([peer(2), peer(1), peer(2)]),
            NetworkTarget::AllExcept(PeerSet::from([peer(1), peer(2)]))
        );
    }

    /// A clone shares its peer list.
    #[test]
    fn a_clone_shares_the_peer_list() {
        let target = NetworkTarget::from([peer(0), peer(1)]);
        let (NetworkTarget::Only(original), NetworkTarget::Only(clone)) = (&target, target.clone())
        else {
            panic!("a list of peers is `Only`");
        };
        assert!(core::ptr::eq(original.as_ptr(), clone.as_ptr()));
        assert_eq!(size_of::<NetworkTarget>(), 24);
    }

    #[test]
    fn targets_matches_each_variant() {
        let cases = [
            (NetworkTarget::None, [false, false]),
            (NetworkTarget::All, [true, true]),
            (NetworkTarget::single(peer(0)), [true, false]),
            (NetworkTarget::all_except([peer(0)]), [false, true]),
            (NetworkTarget::from([peer(1)]), [false, true]),
            (NetworkTarget::all_except([peer(1)]), [true, false]),
        ];
        for (target, expected) in cases {
            assert_eq!(
                [target.targets(&peer(0)), target.targets(&peer(1))],
                expected,
                "{target:?}"
            );
        }
    }
}
