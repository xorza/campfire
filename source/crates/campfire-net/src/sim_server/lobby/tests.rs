use std::mem;

use bevy_ecs::system::RunSystemOnce;
use campfire_common::MapName;
use campfire_package::PackageDir;
use campfire_protocol::internals::TestKey;
use campfire_protocol::secp256k1::{All, Secp256k1};
use campfire_protocol::{
    CertificateHash, ConnectChallenge, ConnectError, DelegationError, DelegationTerms,
    SeedContribution,
};
use lightyear::prelude::{PeerId, RemoteId};

use super::*;
use crate::join::Join;
use crate::order_script::OrderScript;
use crate::session_times::SessionTimes;
use crate::sim_server::server_bots::SlotBot;

const NOW: u64 = 1_700_000_000;

/// A session of the lane mode's 2 slots, with bots in slots `bots` and slots `open` open.
fn setup(bots: &[u32], open: &[u32]) -> LobbySetup {
    let script = OrderScript::parse("end = 1").unwrap();
    LobbySetup {
        packages: Arc::new(
            ModePackages::from_dir(
                &PackageDir::workspace("test/modes/lane"),
                &MapName::new("lane").unwrap(),
            )
            .unwrap(),
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

/// What a lobby's players join with: the terms their delegations grant, and the challenge it
/// offered them.
#[derive(Debug)]
struct Joining {
    secp: Secp256k1<All>,
    granted: DelegationTerms,
    challenge: ConnectChallenge,
}

impl Joining {
    fn new(lobby: &Lobby) -> Joining {
        let granted = DelegationTerms {
            session_key: TestKey::of(2).x_only_public_key().0,
            server_key: TestKey::server().x_only_public_key().0,
            session_id: lobby.terms().session_id(),
            seed_contribution: SeedContribution::new([6; 32]),
            expiration: NOW + 60,
        };
        Joining {
            secp: Secp256k1::new(),
            granted,
            challenge: ConnectChallenge::new([9; 32]),
        }
    }

    /// The delegation of the main key of byte `main` to the session key of byte `session`.
    fn delegation(&self, main: u8, session: u8) -> Delegation {
        let terms = DelegationTerms {
            session_key: TestKey::of(session).x_only_public_key().0,
            ..self.granted
        };
        Delegation::sign(&self.secp, &TestKey::of(main), &terms, NOW, &[0; 32])
    }

    /// A join of `json`, whose answer the session key of byte `session` signs over the
    /// certificate of bytes `certificate`.
    fn join(&self, json: &str, session: u8, certificate: u8) -> Join {
        Join {
            delegation: json.to_owned(),
            answer: self.challenge.answer(
                &self.secp,
                &TestKey::of(session),
                &CertificateHash::new([certificate; 32]),
                &[0; 32],
            ),
        }
    }

    /// The join of the main key of byte `main` from the session key of byte `session`, over this
    /// server's certificate.
    fn signed(&self, main: u8, session: u8) -> Join {
        self.join(self.delegation(main, session).json(), session, 3)
    }
}

fn links(world: &mut World) -> [Entity; 4] {
    [1, 2, 3, 4].map(|peer| {
        let remote = RemoteId(PeerId::Local(peer));
        world.spawn((remote, Connected, Joined)).id()
    })
}

#[test]
fn one_frames_joins_go_in_the_order_of_their_keys_however_a_query_gives_them() {
    // In either order, the answered joins go in the order of their main keys, then their
    // session keys, and the join over another certificate is refused. The x-only keys begin
    // 46… for byte 4 and 53… for byte 3, and the session keys 4d… for byte 2 and f9… for byte 8.
    let lobby = Lobby::new(setup(&[], &[])).unwrap();
    let joining = Joining::new(&lobby);
    let links = links(&mut World::new());
    let frame = [
        (links[0], joining.signed(4, 2)),
        (links[1], joining.signed(3, 8)),
        (
            links[2],
            joining.join(joining.delegation(1, 2).json(), 2, 4),
        ),
        (links[3], joining.signed(3, 2)),
    ];
    let expected = [links[0], links[3], links[1]];
    let mut checked = CheckedJoins::default();
    for reversed in [false, true] {
        let mut joins = frame.clone();
        if reversed {
            joins.reverse();
        }
        let joins = joins
            .into_iter()
            .map(|(link, join)| (link, joining.challenge, join));
        lobby.offering.check_all(joins, &mut checked);
        let answered: Vec<Entity> = checked.answered.drain(..).map(|(link, _)| link).collect();
        assert_eq!(answered, expected, "reversed: {reversed}");
        assert_eq!(
            mem::take(&mut checked.refused),
            [(links[2], JoinError::Connect(ConnectError::BadAnswer))],
            "reversed: {reversed}"
        );
    }
}

#[test]
fn the_lobby_takes_a_join_only_with_a_delegation_and_an_answer_for_it() {
    let mut lobby = Lobby::new(setup(&[], &[])).unwrap();
    let joining = Joining::new(&lobby);
    let challenge = joining.challenge;
    assert_eq!((lobby.players, lobby.joined()), (2, 0));
    let delegation = joining.delegation(1, 2);
    let join = |certificate: u8, json: &str| joining.join(json, 2, certificate);
    assert_eq!(
        lobby.offering.check(challenge, &join(3, delegation.json())),
        Ok(delegation.clone())
    );
    // An answer over another certificate, as a relay to this server gives, and a delegation
    // that is not an event.
    assert_eq!(
        lobby.offering.check(challenge, &join(4, delegation.json())),
        Err(JoinError::Connect(ConnectError::BadAnswer))
    );
    assert!(matches!(
        lobby.offering.check(challenge, &join(3, "{}")),
        Err(JoinError::Delegation(DelegationError::NotEvent(_)))
    ));

    // Two seats: a refused join takes none; a second join of one main key, from another link,
    // takes its seat back, and gives its older link; and a third player finds both taken.
    let take = |lobby: &mut Lobby, link: Entity, join: &Join| {
        let delegation = lobby.offering.check(challenge, join)?;
        lobby.seat(link, delegation)
    };
    let mut world = World::new();
    let links = links(&mut world);
    let good = join(3, delegation.json());
    let player = |main: u8| joining.signed(main, 2);
    assert!(take(&mut lobby, links[0], &join(4, delegation.json())).is_err());
    assert_eq!(lobby.joined(), 0);
    assert_eq!(take(&mut lobby, links[2], &good), Ok(None));
    assert_eq!(take(&mut lobby, links[0], &good), Ok(Some(links[2])));
    assert_eq!(take(&mut lobby, links[1], &player(3)), Ok(None));
    assert_eq!(take(&mut lobby, links[2], &player(4)), Err(JoinError::Full));
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
    assert_eq!(take(&mut lobby, links[3], &player(5)), Ok(None));
    assert_eq!(seated(&lobby), [links[1], links[3]]);
}
