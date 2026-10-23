use super::*;

fn peer(id: u64) -> PeerId {
    PeerId::Entity(id)
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
