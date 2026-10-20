//! Players whose links fail and come back, who log in again, whose delegations expire, who stay
//! away past the grace period, and who join late, each on the server's door.

use std::time::Duration;

use bevy_ecs::query::With;
use campfire_bots::OrderScript;
use campfire_common::{AfterLeave, Seat};
use campfire_math::Num;
use campfire_mode::PlayersData;
use campfire_net::internals::{SimServerWorldInternals, TickHashes};
use campfire_net::{
    CloseCode, InputsDiscarded, JoinError, JoinRefused, JoinState, LinkLossCause, LinkLost, Loss,
    SessionTimes,
};
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_net_link::Unlinked;
use campfire_orders::OrdersDirective;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Controller, LeaveReason, ServerInput};
use campfire_runner::Session;
use campfire_sim::StableId;
use campfire_store::internals::TestDir;
use campfire_units::{Destination, GroundPoint, Owner};

/// Steps a client and the server may take to link, offer, join and play again.
const REJOIN_STEPS: usize = 300;

const fn move_to(x: i64, z: i64) -> OrdersDirective {
    OrdersDirective::Move {
        to: GroundPoint {
            x: Num::int(x),
            z: Num::int(z),
        },
    }
}

fn is_playing(local: &InProcessMatch, client: usize) -> bool {
    local
        .client(client)
        .world()
        .resource::<JoinState>()
        .playing_clock()
        .is_some()
}

/// Steps `local` until `done` holds, at most `steps` times.
fn step_until(local: &mut InProcessMatch, steps: usize, done: impl Fn(&InProcessMatch) -> bool) {
    for _ in 0..steps {
        if done(local) {
            return;
        }
        local.step();
    }
    assert!(done(local), "not done in {steps} steps");
}

/// The stable ids of the units `client`'s world holds, in order.
fn units(local: &mut InProcessMatch, client: usize) -> Vec<u64> {
    let world = local.client_mut(client).world_mut();
    let mut ids: Vec<u64> = world
        .query::<&StableId>()
        .iter(world)
        .map(|id| id.get())
        .collect();
    ids.sort_unstable();
    ids
}

/// The slot the server seated `client`'s link in.
fn slot_of(local: &InProcessMatch, client: usize) -> Seat {
    local
        .server()
        .world()
        .link_seat(local.link(client))
        .unwrap()
}

/// Who controls `seat` on the server.
fn controller(local: &InProcessMatch, seat: Seat) -> Option<Controller<'_>> {
    let session = local.server().world().resource::<Session>();
    session.log().controller(seat)
}

/// Where the server's hero of `seat` walks to.
fn destination(local: &mut InProcessMatch, seat: Seat) -> Destination {
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<(&Owner, &Destination), With<StableId>>();
    let (_, destination) = heroes
        .iter(world)
        .find(|(owner, _)| owner.seat() == seat)
        .unwrap();
    *destination
}

/// Whether the server's hero of `seat` walks somewhere within 20 steps.
fn moves(local: &mut InProcessMatch, seat: Seat) -> bool {
    for _ in 0..20 {
        if destination(local, seat) != Destination::default() {
            return true;
        }
        local.step();
    }
    false
}

#[test]
fn a_client_whose_link_is_cut_comes_back_within_the_grace_period() {
    let mut local = InProcessMatch::new(InProcessSetup::SOLO);
    local.start_match();
    for _ in 0..20 {
        local.step();
    }
    let seat = slot_of(&local, 0);
    assert!(!units(&mut local, 0).is_empty());

    // An order the client sends as its link is cut never reaches the server. The server logs the
    // seat disconnected; the client tries again, and the network stack despawned what the link
    // carried.
    local.order(0, move_to(3, -2));
    local.client_frame(0);
    local.cut_link(0);
    let disconnected = ServerInput::Disconnected { seat };
    step_until(&mut local, 10, |local| {
        local.server_inputs() == [disconnected.clone()]
    });
    assert!(
        local
            .client(0)
            .world()
            .resource::<JoinState>()
            .is_rejoining()
    );
    assert_eq!(units(&mut local, 0), Vec::<u64>::new());

    // The network works again: the client links, joins, and plays on from the server's copy of
    // its chain, which lacks the order, and the server logs it connected.
    local.mend_link(0);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, 0));
    assert_eq!(
        local.server_inputs(),
        [disconnected, ServerInput::Connected { seat }]
    );
    assert_eq!(
        local.log().take::<InputsDiscarded>(),
        [InputsDiscarded { seat, count: 1 }]
    );
    // Its world holds each unit once, which the new link carried.
    for _ in 0..10 {
        local.step();
    }
    let ids = units(&mut local, 0);
    let mut once = ids.clone();
    once.dedup();
    assert!(!ids.is_empty());
    assert_eq!(ids, once);
    // An order after the rejoin moves the hero on the server.
    local.order(0, move_to(-2, 5));
    assert!(moves(&mut local, seat));
}

#[test]
fn a_second_login_takes_the_slot_and_ends_the_first_link() {
    let mut local = InProcessMatch::new(InProcessSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // A second client of the same player joins while the first link lives: it takes the slot,
    // and the first client stops, told why.
    let second = local.add_client(0);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, second));
    assert_eq!(slot_of(&local, second), slot);
    let loss = |local: &InProcessMatch| local.client(0).world().resource::<JoinState>().loss();
    step_until(&mut local, 5, |local| loss(local).is_some());
    assert_eq!(loss(&local), Some(Loss::Superseded));
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            cause: LinkLossCause::ClientClosed(CloseCode::Superseded)
        }]
    );
    assert!(local.server_inputs().is_empty());
    assert!(local.server().world().link_seat(local.link(0)).is_none());
}

#[test]
fn of_two_logins_in_one_frame_the_later_takes_the_slot() {
    let mut local = InProcessMatch::new(InProcessSetup::SOLO);
    assert_eq!(local.setup().server_frames, 1, "a step is one server frame");
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // Two more clients of the same player log in through equal links, so the server checks both
    // answers in one frame: the later one in its order takes the slot and dismisses the earlier,
    // which it seated in the same frame, as `SeatLinks` records the link it seated last.
    let logins = [local.add_client(0), local.add_client(0)];
    let seated = |local: &InProcessMatch, client: usize| {
        local.server().world().link_seat(local.link(client))
    };
    step_until(&mut local, REJOIN_STEPS, |local| {
        logins.iter().any(|&client| seated(local, client).is_some())
    });
    let held: Vec<usize> = logins
        .into_iter()
        .filter(|&client| seated(&local, client) == Some(slot))
        .collect();
    let [winner] = held[..] else {
        panic!("one login holds the slot: {held:?}");
    };
    let loser = logins[usize::from(winner == logins[0])];
    // No login held the slot in the step before, and the loser does not take it after: the
    // server answered both in this one frame.
    let loss = |local: &InProcessMatch, client: usize| {
        local.client(client).world().resource::<JoinState>().loss()
    };
    step_until(&mut local, 20, |local| {
        is_playing(local, winner)
            && [0, loser]
                .iter()
                .all(|&client| loss(local, client).is_some())
    });
    assert_eq!(loss(&local, 0), Some(Loss::Superseded));
    assert_eq!(loss(&local, loser), Some(Loss::Superseded));
    let superseded = LinkLost {
        cause: LinkLossCause::ClientClosed(CloseCode::Superseded),
    };
    assert_eq!(local.log().take::<LinkLost>(), [superseded, superseded]);
    assert_eq!(seated(&local, winner), Some(slot));
    assert_eq!(seated(&local, loser), None);
    assert!(local.server_inputs().is_empty());
}

#[test]
fn a_superseded_client_hears_of_it_though_the_first_notice_is_lost() {
    let mut local = InProcessMatch::new(InProcessSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // The server's packets to the first client are lost as a second login takes the slot: the
    // server keeps the old link, as its notice has not arrived, and the first client plays on.
    local.lose_server_packets(0);
    let second = local.add_client(0);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, second));
    let loss = |local: &InProcessMatch| local.client(0).world().resource::<JoinState>().loss();
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(loss(&local), None);
    let first = local.link(0);
    assert!(local.server().world().get::<Unlinked>(first).is_none());
    // The packets pass again: the reliable channel sends the notice again, and the first client
    // stops and ends its link. The newer login keeps the slot, and no input is logged.
    local.pass_server_packets(0);
    step_until(&mut local, 20, |local| loss(local).is_some());
    assert_eq!(loss(&local), Some(Loss::Superseded));
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            cause: LinkLossCause::ClientClosed(CloseCode::Superseded)
        }]
    );
    assert_eq!(slot_of(&local, second), slot);
    assert!(is_playing(&local, second));
    assert!(local.server_inputs().is_empty());
}

#[test]
fn an_expiring_delegation_is_renewed_as_the_client_comes_back() {
    let mut local = InProcessMatch::new(InProcessSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // A day on, the delegation expires: the client, its link cut and back, delegates a new
    // session key, and the server logs the renewal before the slot comes back.
    local.set_clock(TestKey::NOW + 86_400 + 10);
    local.cut_link(0);
    local.step();
    local.mend_link(0);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, 0));
    let inputs = local.server_inputs();
    assert!(
        matches!(
            inputs.as_slice(),
            [
                ServerInput::Disconnected { seat: a },
                ServerInput::Renew { seat: b, .. },
                ServerInput::Connected { seat: c },
            ] if [*a, *b, *c] == [slot; 3]
        ),
        "{inputs:?}"
    );
    // The new key signs the chain from then on: an order moves the hero.
    local.order(0, move_to(-2, 5));
    assert!(moves(&mut local, slot));
}

/// A two-player match under `rules`, its grace period a second, whose client 1 is gone past it;
/// with its slot.
fn left_past_grace(rules: PlayersData) -> (InProcessMatch, Seat) {
    let mut setup = InProcessSetup::duo(LinkModel::perfect(), InProcessMatch::SEED_CHAIN);
    setup.rules = rules;
    setup.times = SessionTimes {
        grace: Duration::from_secs(1),
        restore_window: Duration::from_secs(1),
    };
    let mut local = InProcessMatch::new(setup);
    local.start_match();
    let slot = slot_of(&local, 1);
    local.cut_link(1);
    // 30 ticks a second: past the second of grace.
    step_until(&mut local, 40, |local| {
        local
            .server_inputs()
            .iter()
            .any(|input| matches!(input, ServerInput::Leave { .. }))
    });
    (local, slot)
}

#[test]
fn a_player_gone_past_the_grace_period_leaves_as_the_mode_says_and_a_late_joiner_comes_in() {
    let rules = |leaver, late_join, bot_takeover| PlayersData {
        late_join,
        bot_takeover,
        leaver,
        ..PlayersData::DEFAULT
    };
    // Reserved: a new player finds no slot, and the leaver takes theirs back, on a new chain.
    let (mut local, seat) = left_past_grace(rules(AfterLeave::Reserve, false, false));
    assert_eq!(
        local.server_inputs()[1..],
        [ServerInput::Leave {
            seat,
            reason: LeaveReason::Grace,
            becomes: AfterLeave::Reserve,
        }]
    );
    assert_eq!(controller(&local, seat), Some(Controller::Reserved));
    let late = local.add_client(2);
    for _ in 0..60 {
        local.step();
    }
    // The server refuses the new player's join, and tells their client, which stops.
    let refused: Vec<JoinError> = local
        .log()
        .take::<JoinRefused>()
        .iter()
        .map(|refused| refused.error)
        .collect();
    assert_eq!(refused, [JoinError::NoSeat]);
    assert!(!is_playing(&local, late));
    let state = local.client(late).world().resource::<JoinState>();
    assert_eq!(state.loss(), Some(Loss::JoinRefused));
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            cause: LinkLossCause::ClientClosed(CloseCode::JoinRefused)
        }]
    );
    // The leaver's client gave up as the grace period and the restore window passed, and the
    // player comes back in a new client.
    let back = local.add_client(1);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, back));
    assert!(matches!(
        local.server_inputs().last(),
        Some(ServerInput::Join { seat: joined, .. }) if *joined == seat
    ));

    // Played by a bot, which a new player takes over.
    let (mut local, slot) = left_past_grace(rules(AfterLeave::Bot, true, true));
    assert_eq!(controller(&local, slot), Some(Controller::Bot));
    let late = local.add_client(2);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, late));
    assert_eq!(slot_of(&local, late), slot);

    // Open, which a new player takes.
    let (mut local, slot) = left_past_grace(rules(AfterLeave::Open, true, false));
    assert_eq!(controller(&local, slot), Some(Controller::Open));
    let late = local.add_client(2);
    step_until(&mut local, REJOIN_STEPS, |local| is_playing(local, late));
    assert_eq!(slot_of(&local, late), slot);
    assert!(local.log().take::<LinkLost>().len() <= 1);
}

#[test]
fn every_client_rejoins_a_restored_server() {
    let test_dir = TestDir::new();
    let mut local = InProcessMatch::new(InProcessSetup::duo(
        LinkModel::perfect(),
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(test_dir.path("data"));
    local.start_match();
    for _ in 0..30 {
        local.step();
    }
    let slots = [0, 1].map(|client| slot_of(&local, client));
    let before = local
        .server()
        .world()
        .resource::<TickHashes>()
        .get()
        .to_vec();
    local.restart_server();
    // An order due in the tick the server runs on from, which each client plays as soon as it
    // plays again, late. It takes effect: the client stamps it once its timeline counts the
    // restored server's ticks, which start from 0 again, and not the stopped server's, which run
    // ahead past the max input lead.
    let script = format!(
        "[[order]]\ntick = {}\ndirective.orders.move.to = [3, -2]\n",
        local.next_tick(End::Server)
    );
    for client in [0, 1] {
        local.play(client, &OrderScript::parse(&script).unwrap());
    }
    step_until(&mut local, REJOIN_STEPS, |local| {
        is_playing(local, 0) && is_playing(local, 1)
    });
    let world = local.server().world();
    assert_eq!(world.resource::<TickHashes>().get()[..before.len()], before);
    let inputs = local.server_inputs();
    for seat in slots {
        assert!(inputs.contains(&ServerInput::Disconnected { seat }));
        assert!(inputs.contains(&ServerInput::Connected { seat }));
    }
    assert!(local.next_tick(End::Server) > u64::try_from(before.len()).unwrap());
    let walking = |local: &mut InProcessMatch| {
        slots.map(|slot| destination(local, slot) != Destination::default())
    };
    for _ in 0..REJOIN_STEPS {
        if walking(&mut local) == [true; 2] {
            break;
        }
        local.step();
    }
    assert_eq!(walking(&mut local), [true; 2]);
    drop(local);
    drop(test_dir);
}
