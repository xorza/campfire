use std::mem;
use std::time::Duration;

use bevy_ecs::system::RunSystemOnce;
use campfire_bots::OrderScript;
use campfire_common::{ChannelBinding, DeclaredName};
use campfire_host::Os;
use campfire_host::PlannedBot;
use campfire_host::SessionTimes;
use campfire_host::UnixClock;
use campfire_host::{Join, JoinRole};
use campfire_mode::{PickError, PlayerSlotPickError};
use campfire_net_connection::RemoteId;
use campfire_net_core::PeerId;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    ConnectAnswer, ConnectChallenge, ConnectError, DelegationError, DelegationTerms, Entropy,
    SeedContribution, Signer,
};
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;
use campfire_values::Scalar;
use campfire_visibility::DataKind;
use campfire_visibility::{OutView, Setting, SpectatorFog, Visibility, VisibilityError};

use super::*;
use crate::sim_server::offering::Offered;

/// A session of the lane mode's 2 player slots, with bots in player slots `bots` and player slots
/// `open` open.
fn setup(bots: &[u32], open: &[u32]) -> LobbySetup {
    LobbySetup {
        opening: opening(bots, open),
        seed_chain: SeedChain::new([7; 32], NonZeroU32::MIN),
        tick_hz: NonZeroU32::new(30).unwrap(),
        inputs: InputRules::LAN,
        player_slots: 2,
    }
}

/// The opening of a session of the lane mode, with bots in player slots `bots` and player slots
/// `open` open.
fn opening(bots: &[u32], open: &[u32]) -> LobbyOpening {
    let script = OrderScript::parse("end = 1").unwrap();
    let packages = TestMode::Lane.packages();
    let player_slots = bots
        .iter()
        .map(|&player_slot| PlannedBot::orders(player_slot, script.clone()));
    LobbyOpening {
        bots: ServerBots::new(player_slots, None, &packages).unwrap(),
        packages: Arc::new(packages),
        open: open.iter().copied().map(PlayerSlot::new).collect(),
        host: HostVisibility::default(),
        options: Vec::new(),
        player_slot_picks: Vec::new(),
        server: ServerSetup {
            signer: Arc::new(Signer::new(TestKey::server(), Entropy::Fixed(5))),
            times: SessionTimes::DEFAULT,
            os: Os {
                clock: UnixClock::manual(TestKey::NOW),
                entropy: Entropy::Fixed(5),
            },
        },
        start: LobbyStart::Players,
    }
}

#[test]
fn the_lobby_plans_each_player_slot_once_and_leaves_one_to_a_player_unless_it_is_watched() {
    let plan = |bots: &[u32], open: &[u32]| {
        Lobby::new(setup(bots, open)).map(|lobby| lobby.terms().player_slots.clone())
    };
    assert_eq!(
        plan(&[1], &[]),
        Ok(vec![PlayerSlotPlan::Player, PlayerSlotPlan::Bot])
    );
    assert_eq!(
        plan(&[], &[0]),
        Ok(vec![PlayerSlotPlan::Open, PlayerSlotPlan::Player])
    );
    let player_slot = PlayerSlot::new;
    assert_eq!(
        plan(&[2], &[]),
        Err(LobbyError::NoSuchPlayerSlot(player_slot(2)))
    );
    assert_eq!(
        plan(&[1, 1], &[]),
        Err(LobbyError::PlayerSlotNamedTwice(player_slot(1)))
    );
    assert_eq!(
        plan(&[1], &[1]),
        Err(LobbyError::PlayerSlotNamedTwice(player_slot(1)))
    );
    assert_eq!(plan(&[0], &[1]), Err(LobbyError::NoPlayer));
    // A local watch holds bots alone.
    let mut watched = setup(&[0], &[1]);
    watched.opening.start = LobbyStart::Watched;
    let watched = Lobby::new(watched).map(|lobby| lobby.terms().player_slots.clone());
    assert_eq!(watched, Ok(vec![PlayerSlotPlan::Bot, PlayerSlotPlan::Open]));
}

#[test]
fn the_lobby_takes_the_hosts_visibility_where_the_mode_leaves_it() {
    // The lane mode sets no visibility: it leaves its host the spectators' fog and what a player
    // out of play sees, and no kind.
    let hosted = |settings: &[&str], max_spectators| {
        let mut setup = setup(&[], &[]);
        setup.opening.host = HostVisibility {
            settings: settings.iter().map(|text| text.parse().unwrap()).collect(),
            max_spectators: Some(max_spectators),
        };
        Lobby::new(setup).map(|lobby| {
            let terms = Visibility::decode(&lobby.terms().visibility).unwrap();
            assert_eq!(terms, lobby.checked().visibility);
            terms
        })
    };
    let visibility = hosted(&["spectators=teams", "out=spectators"], 2).unwrap();
    assert_eq!(visibility.spectators(), SpectatorFog::Teams);
    assert_eq!(visibility.out(), OutView::Spectators);
    assert_eq!(visibility.max_spectators(), 2);
    assert_eq!(hosted(&[], 0), Ok(Visibility::DEFAULT));
    let inventory = Setting::Kind(DataKind::Inventory);
    assert_eq!(
        hosted(&["inventory=allies"], 0),
        Err(LobbyError::Visibility(VisibilityError::NotLeft(inventory)))
    );
    assert_eq!(
        hosted(&["out=spectators", "out=team"], 0),
        Err(LobbyError::Visibility(VisibilityError::Repeated(
            Setting::Out
        )))
    );
}

#[test]
fn the_lobby_records_the_hosts_picks_of_the_modes_options() {
    let picked = |packages: ModePackages, player_slots, options: &[&str]| {
        let mut setup = setup(&[], &[]);
        setup.opening.packages = Arc::new(packages);
        setup.player_slots = player_slots;
        setup.opening.options = options.iter().map(|text| text.parse().unwrap()).collect();
        Lobby::new(setup).map(|lobby| lobby.terms().options.clone())
    };
    // The skirmish's 3 player slots, its money of 100 by default, or the 5,000 its host picks, its
    // second value.
    let skirmish = || TestMode::Skirmish.packages();
    assert_eq!(picked(skirmish(), 3, &[]), Ok(vec![0]));
    assert_eq!(picked(skirmish(), 3, &["start_money=5000"]), Ok(vec![1]));
    assert_eq!(
        picked(skirmish(), 3, &["start_money=7"]),
        Err(LobbyError::Options(PickError::NoValue {
            option: DeclaredName::new("start_money").unwrap(),
            value: Scalar::Int(7),
        }))
    );
    // The lane declares no option: a pick of one is refused.
    assert_eq!(
        picked(TestMode::Lane.packages(), 2, &["start_money=5000"]),
        Err(LobbyError::Options(PickError::Unknown(
            DeclaredName::new("start_money").unwrap()
        )))
    );
    // The lane declares no choice of values: a player slot's pick of one is refused, and none makes
    // no player slot pick in the terms.
    let mut setup = setup(&[], &[]);
    setup.opening.player_slot_picks = vec!["0:faction=china".parse().unwrap()];
    assert_eq!(
        Lobby::new(setup).err(),
        Some(LobbyError::PlayerSlotPicks(PlayerSlotPickError::Unknown(
            DeclaredName::new("faction").unwrap()
        )))
    );
    let lobby = Lobby::new(self::setup(&[], &[])).unwrap();
    assert!(lobby.terms().player_slot_picks.is_empty());
}

/// What a lobby's players join with: the terms their delegations grant, and the challenge it
/// offered them over their links' channel binding, of bytes 3.
#[derive(Debug)]
struct Joining {
    granted: DelegationTerms,
    offered: Offered,
}

impl Joining {
    fn new(lobby: &Lobby) -> Self {
        let granted = DelegationTerms {
            session_key: TestKey::of(2).x_only_public_key().0,
            server_key: TestKey::server().x_only_public_key().0,
            session_id: lobby.terms().session_id(),
            seed_contribution: SeedContribution::new([6; 32]),
            expiration: TestKey::NOW + 60,
        };
        Self {
            granted,
            offered: Offered {
                challenge: ConnectChallenge::new([9; 32]),
                binding: ChannelBinding::new([3; 32]),
                sent: Duration::ZERO,
                first: Duration::ZERO,
            },
        }
    }

    /// The delegation of the main key of byte `main` to the session key of byte `session`.
    fn delegation(&self, main: u8, session: u8) -> Delegation {
        let terms = DelegationTerms {
            session_key: TestKey::of(session).x_only_public_key().0,
            ..self.granted
        };
        Delegation::sign(&TestKey::signer(TestKey::of(main)), &terms, TestKey::NOW)
    }

    /// A join of `json`, whose answer the session key of byte `session` signs over the
    /// channel binding of bytes `binding`.
    fn join(&self, json: &str, session: u8, binding: u8) -> Join {
        Join {
            delegation: json.to_owned(),
            answer: TestKey::signer(TestKey::of(session)).sign(&ConnectAnswer {
                challenge: &self.offered.challenge,
                binding: &ChannelBinding::new([binding; 32]),
            }),
            role: JoinRole::Play,
        }
    }

    /// The join of the main key of byte `main` from the session key of byte `session`, over the
    /// link's binding.
    fn signed(&self, main: u8, session: u8) -> Join {
        self.join(self.delegation(main, session).json(), session, 3)
    }
}

fn links(world: &mut World) -> [Entity; 4] {
    [1, 2, 3, 4].map(|peer| {
        let remote = RemoteId(PeerId::Entity(peer));
        world.spawn((remote, Connected, Joined)).id()
    })
}

#[test]
fn one_frames_joins_go_in_the_order_of_their_keys_however_a_query_gives_them() {
    // In either order, the answered joins go in the order of their main keys, then their
    // session keys, and the join over another session's binding is refused. The x-only keys begin
    // 46… for byte 4 and 53… for byte 3, and the session keys 4d… for byte 2 and f9… for byte 8.
    let mut lobby = Lobby::new(setup(&[], &[])).unwrap();
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
            .map(|(link, join)| (link, joining.offered, join));
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
    let offered = joining.offered;
    assert_eq!((lobby.players, lobby.joined()), (2, 0));
    let delegation = joining.delegation(1, 2);
    let join = |binding: u8, json: &str| joining.join(json, 2, binding);
    assert_eq!(
        lobby.offering.check(&offered, &join(3, delegation.json())),
        Ok(delegation.clone())
    );
    // An answer over another TLS session's binding, as a relay between two sessions gives, and a
    // delegation that is not an event.
    assert_eq!(
        lobby.offering.check(&offered, &join(4, delegation.json())),
        Err(JoinError::Connect(ConnectError::BadAnswer))
    );
    assert!(matches!(
        lobby.offering.check(&offered, &join(3, "{}")),
        Err(JoinError::Delegation(DelegationError::NotEvent(_)))
    ));

    // Two player slots: a refused join takes none; a second join of one main key, from another
    // link, takes its player slot back, and gives its older link; and a third player finds both
    // taken.
    let take = |lobby: &mut Lobby, link: Entity, join: &Join| {
        let delegation = lobby.offering.check(&offered, join)?;
        lobby.player_slot(link, delegation)
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
    let held =
        |lobby: &Lobby| -> Vec<Entity> { lobby.joined.iter().map(|&(link, _)| link).collect() };
    assert_eq!(held(&lobby), links[..2]);

    // The first player's link disconnects: their player slot is free, their link no longer joined,
    // and a fourth player takes the player slot.
    world.entity_mut(links[0]).remove::<Connected>();
    world.insert_resource(lobby);
    world.run_system_once(Lobby::free_player_slots).unwrap();
    assert!(!world.entity(links[0]).contains::<Joined>());
    let mut lobby = world.remove_resource::<Lobby>().unwrap();
    assert_eq!(held(&lobby), [links[1]]);
    assert_eq!(take(&mut lobby, links[3], &player(5)), Ok(None));
    assert_eq!(held(&lobby), [links[1], links[3]]);
}

#[test]
fn a_servers_new_session_opens_every_player_slot_by_the_lan_rules_and_keeps_its_files() {
    let test_dir = TestDir::new();
    let data = ServerDir::open(&test_dir.path("data")).unwrap();
    let opening = |bots: &[u32]| opening(bots, &[]);
    // The lane mode's 2 player slots, both players', at its default and only rate of 30 Hz; the
    // seed chain of 1024 segments from the server's entropy, every byte 5.
    let lobby = Lobby::open(opening(&[]), &data).unwrap();
    let terms = lobby.terms();
    assert_eq!(
        terms.player_slots,
        [PlayerSlotPlan::Player, PlayerSlotPlan::Player]
    );
    assert_eq!(terms.tick_hz.get(), 30);
    assert_eq!(terms.inputs, InputRules::LAN);
    let seed_chain = SeedChain::new([5; 32], NonZeroU32::new(1024).unwrap());
    assert_eq!(terms.seed_commitment, seed_chain.commitment());
    assert!(lobby.files.is_some());
    // A bot of a player slot the mode does not have opens no session.
    assert!(matches!(
        Lobby::open(opening(&[2]), &data),
        Err(LobbyOpenError::Lobby(LobbyError::NoSuchPlayerSlot(player_slot))) if player_slot.get() == 2
    ));
}
