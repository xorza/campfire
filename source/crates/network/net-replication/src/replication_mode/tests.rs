use campfire_net_core::PeerId;

use super::*;

/// Each mode admits exactly its links, and none without a `RemoteId`.
#[test]
fn each_mode_selects_its_links() {
    let [a, b] = [Entity::from_bits(1), Entity::from_bits(2)];
    let [remote_a, remote_b] = [RemoteId(PeerId::Entity(1)), RemoteId(PeerId::Entity(2))];
    let modes = [
        (ReplicationMode::default(), [true, true]),
        (
            ReplicationMode::Target(NetworkTarget::single(PeerId::Entity(2))),
            [false, true],
        ),
        (
            ReplicationMode::Links(SmallVec::from_slice(&[a])),
            [true, false],
        ),
    ];
    for (mode, expected) in modes {
        assert_eq!(
            [
                mode.is_visible_for(a, Some(&remote_a)),
                mode.is_visible_for(b, Some(&remote_b)),
            ],
            expected,
            "{mode:?}"
        );
        assert!(!mode.is_visible_for(a, None), "{mode:?}");
    }
}
