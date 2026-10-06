use bevy_ecs::system::RunSystemOnce;
use campfire_package::PackageDir;
use campfire_protocol::internals::TestKey;
use campfire_protocol::secp256k1::Secp256k1;
use campfire_protocol::{
    CertificateHash, ConnectError, DelegationError, DelegationTerms, SeedContribution,
};
use lightyear::prelude::{PeerId, RemoteId};

use super::*;
use crate::order_script::OrderScript;
use crate::session_times::SessionTimes;
use crate::sim_server::server_bots::SlotBot;

const NOW: u64 = 1_700_000_000;

/// A session of the lane mode's 2 slots, with bots in slots `bots` and slots `open` open.
fn setup(bots: &[u32], open: &[u32]) -> LobbySetup {
    let script = OrderScript::parse("end = 1").unwrap();
    LobbySetup {
        packages: Arc::new(
            ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap(),
        ),
        seed_chain: SeedChain::new([7; 32], NonZeroU32::MIN),
        tick_hz: NonZeroU32::new(30).unwrap(),
        inputs: InputRules::LAN,
        slots: 2,
        bots: ServerBots {
            slots: bots
                .iter()
                .map(|&slot| SlotBot::new(slot, script.clone()))
                .collect(),
            takeover: None,
        },
        open: open.iter().copied().map(PlayerSlot::new).collect(),
        server: ServerSetup {
            key: TestKey::server(),
            certificate: CertificateHash::new([3; 32]),
            times: SessionTimes::DEFAULT,
            clock: || NOW,
            entropy: |bytes| bytes.fill(5),
        },
    }
}

#[test]
fn the_lobby_plans_each_slot_once_and_leaves_one_to_a_player() {
    let plan = |bots: &[u32], open: &[u32]| {
        Lobby::new(setup(bots, open)).map(|lobby| lobby.terms().slots.clone())
    };
    assert_eq!(plan(&[1], &[]), Ok(vec![SlotPlan::Player, SlotPlan::Bot]));
    assert_eq!(plan(&[], &[0]), Ok(vec![SlotPlan::Open, SlotPlan::Player]));
    let slot = PlayerSlot::new;
    assert_eq!(plan(&[2], &[]), Err(LobbyError::NoSuchSlot(slot(2))));
    assert_eq!(plan(&[1, 1], &[]), Err(LobbyError::SlotNamedTwice(slot(1))));
    assert_eq!(plan(&[1], &[1]), Err(LobbyError::SlotNamedTwice(slot(1))));
    assert_eq!(plan(&[0], &[1]), Err(LobbyError::NoPlayer));
}

#[test]
fn the_lobby_takes_a_join_only_with_a_delegation_and_an_answer_for_it() {
    let server_key = TestKey::server().x_only_public_key().0;
    let mut lobby = Lobby::new(setup(&[], &[])).unwrap();
    assert_eq!((lobby.players, lobby.joined()), (2, 0));
    let secp = Secp256k1::new();
    let granted = DelegationTerms {
        session_key: TestKey::of(2).x_only_public_key().0,
        server_key,
        session_id: lobby.terms().session_id(),
        seed_contribution: SeedContribution::new([6; 32]),
        expiration: NOW + 60,
    };
    let delegation = Delegation::sign(&secp, &TestKey::of(1), &granted, NOW, &[0; 32]);
    let challenge = ConnectChallenge::new([9; 32]);
    let join = |certificate: [u8; 32], json: &str| Join {
        delegation: json.to_owned(),
        answer: challenge.answer(
            &secp,
            &TestKey::of(2),
            &CertificateHash::new(certificate),
            &[0; 32],
        ),
    };
    assert_eq!(
        lobby
            .offering
            .check(challenge, &join([3; 32], delegation.json())),
        Ok(delegation.clone())
    );
    // An answer over another certificate, as a relay to this server gives, and a delegation
    // that is not an event.
    assert_eq!(
        lobby
            .offering
            .check(challenge, &join([4; 32], delegation.json())),
        Err(JoinError::Connect(ConnectError::BadAnswer))
    );
    assert_eq!(
        lobby.offering.check(challenge, &join([3; 32], "{}")),
        Err(JoinError::Delegation(DelegationError::NotEvent))
    );

    // Two seats: a refused join takes none; a second join of one main key, from another link,
    // takes its seat back, and gives its older link; and a third player finds both taken.
    let good = join([3; 32], delegation.json());
    let player = |main: u8| {
        let delegation = Delegation::sign(&secp, &TestKey::of(main), &granted, NOW, &[0; 32]);
        join([3; 32], delegation.json())
    };
    let mut world = World::new();
    let links = [1, 2, 3, 4].map(|peer| {
        let remote = RemoteId(PeerId::Local(peer));
        world.spawn((remote, Connected, Joined)).id()
    });
    assert!(
        lobby
            .take(links[0], challenge, &join([4; 32], delegation.json()))
            .is_err()
    );
    assert_eq!(lobby.joined(), 0);
    assert_eq!(lobby.take(links[2], challenge, &good), Ok(None));
    assert_eq!(lobby.take(links[0], challenge, &good), Ok(Some(links[2])));
    assert_eq!(lobby.take(links[1], challenge, &player(3)), Ok(None));
    assert_eq!(
        lobby.take(links[2], challenge, &player(4)),
        Err(JoinError::Full)
    );
    let seated =
        |lobby: &Lobby| -> Vec<Entity> { lobby.joined.iter().map(|&(link, _)| link).collect() };
    assert_eq!(seated(&lobby), links[..2]);

    // The first player's link disconnects: their seat is free, their link no longer joined,
    // and a fourth player takes the seat.
    world.entity_mut(links[0]).remove::<Connected>();
    world.insert_resource(lobby);
    world.run_system_once(Lobby::free_seats).unwrap();
    assert!(!world.entity(links[0]).contains::<Joined>());
    let mut lobby = world.remove_resource::<Lobby>().unwrap();
    assert_eq!(seated(&lobby), [links[1]]);
    assert_eq!(lobby.take(links[3], challenge, &player(5)), Ok(None));
    assert_eq!(seated(&lobby), [links[1], links[3]]);
}
