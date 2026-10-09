//! Players whose links fail and come back, who log in again, whose delegations expire, who stay
//! away past the grace period, and who join late, each on the server's door.

use std::time::Duration;

use bevy_ecs::query::With;
use campfire_capabilities::{Action, Destination, Leaver, Owner, PlayersData};
use campfire_common::PlayerSlot;
use campfire_math::Num;
use campfire_net::internals::{End, InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{
    InputsDiscarded, JoinRefused, JoinState, LinkLost, Loss, OrderScript, PlayerLink, SessionTimes,
    TickHashes,
};
use campfire_protocol::{AfterLeave, Controller, LeaveReason, ServerInput};
use campfire_runner::Session;
use campfire_sim::StableId;
use campfire_store::Scratch;
use lightyear::prelude::Unlinked;

/// Steps a client and the server may take to link, offer, join and play again.
const REJOIN_STEPS: usize = 300;

const fn move_to(x: i64, z: i64) -> Action {
    Action::Move {
        x: Num::int(x),
        z: Num::int(z),
    }
}

fn playing(local: &InProcessMatch, client: usize) -> bool {
    local
        .client(client)
        .world()
        .resource::<JoinState>()
        .clock()
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
fn slot_of(local: &InProcessMatch, client: usize) -> PlayerSlot {
    let world = local.server().world();
    world.get::<PlayerLink>(local.link(client)).unwrap().slot()
}

/// Who controls `slot` on the server.
fn controller(local: &InProcessMatch, slot: PlayerSlot) -> Option<Controller> {
    let session = local.server().world().resource::<Session>();
    session.log().controller(slot)
}

/// Where the server's hero of `slot` walks to.
fn destination(local: &mut InProcessMatch, slot: PlayerSlot) -> Destination {
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<(&Owner, &Destination), With<StableId>>();
    let (_, destination) = heroes
        .iter(world)
        .find(|(owner, _)| owner.slot() == slot)
        .unwrap();
    *destination
}

/// Whether the server's hero of `slot` walks somewhere within 20 steps.
fn moves(local: &mut InProcessMatch, slot: PlayerSlot) -> bool {
    for _ in 0..20 {
        if destination(local, slot) != Destination::default() {
            return true;
        }
        local.step();
    }
    false
}

#[test]
fn a_client_whose_link_is_cut_comes_back_within_the_grace_period() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    for _ in 0..20 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    assert!(!units(&mut local, 0).is_empty());

    // An order the client sends as its link is cut never reaches the server. The server logs the
    // slot disconnected; the client tries again, and Lightyear despawned what the link carried.
    local.order(0, move_to(3, -2));
    local.client_frame(0);
    local.cut_link(0);
    let disconnected = ServerInput::Disconnected { slot };
    step_until(&mut local, 10, |local| {
        local.server_inputs() == [disconnected.clone()]
    });
    assert!(local.client(0).world().resource::<JoinState>().rejoining());
    assert_eq!(units(&mut local, 0), Vec::<u64>::new());

    // The network works again: the client links, joins, and plays on from the server's copy of
    // its chain, which lacks the order, and the server logs it connected.
    local.mend_link(0);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, 0));
    assert_eq!(
        local.server_inputs(),
        [disconnected, ServerInput::Connected { slot }]
    );
    assert_eq!(
        local.log().take::<InputsDiscarded>(),
        [InputsDiscarded { slot, count: 1 }]
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
    assert!(moves(&mut local, slot));
}

#[test]
fn a_second_login_takes_the_slot_and_ends_the_first_link() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // A second client of the same player joins while the first link lives: it takes the slot,
    // and the first client stops, told why.
    let second = local.add_client(0);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, second));
    assert_eq!(slot_of(&local, second), slot);
    let loss = |local: &InProcessMatch| local.client(0).world().resource::<JoinState>().loss();
    step_until(&mut local, 5, |local| loss(local).is_some());
    assert_eq!(loss(&local), Some(Loss::Superseded));
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            reason: "a newer login of the player took the slot".to_owned()
        }]
    );
    assert!(local.server_inputs().is_empty());
    assert!(
        local
            .server()
            .world()
            .get::<PlayerLink>(local.link(0))
            .is_none()
    );
}

#[test]
fn a_superseded_client_hears_of_it_though_the_first_notice_is_lost() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // The server's packets to the first client are lost as a second login takes the slot: the
    // server keeps the old link, as its notice has not arrived, and the first client plays on.
    local.lose_server_packets(0, true);
    let second = local.add_client(0);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, second));
    let loss = |local: &InProcessMatch| local.client(0).world().resource::<JoinState>().loss();
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(loss(&local), None);
    let first = local.link(0);
    assert!(local.server().world().get::<Unlinked>(first).is_none());
    // The packets pass again: the reliable channel sends the notice again, and the first client
    // stops and ends its link. The newer login keeps the slot, and no input is logged.
    local.lose_server_packets(0, false);
    step_until(&mut local, 20, |local| loss(local).is_some());
    assert_eq!(loss(&local), Some(Loss::Superseded));
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            reason: "a newer login of the player took the slot".to_owned()
        }]
    );
    assert_eq!(slot_of(&local, second), slot);
    assert!(playing(&local, second));
    assert!(local.server_inputs().is_empty());
}

#[test]
fn an_expiring_delegation_is_renewed_as_the_client_comes_back() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    for _ in 0..10 {
        local.step();
    }
    let slot = slot_of(&local, 0);
    // A day on, the delegation expires: the client, its link cut and back, delegates a new
    // session key, and the server logs the renewal before the slot comes back.
    local.set_clock(1_700_000_000 + 86_400 + 10);
    local.cut_link(0);
    local.step();
    local.mend_link(0);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, 0));
    let inputs = local.server_inputs();
    assert!(
        matches!(
            &inputs[..],
            [
                ServerInput::Disconnected { slot: a },
                ServerInput::Renew { slot: b, .. },
                ServerInput::Connected { slot: c },
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
fn left_past_grace(rules: PlayersData) -> (InProcessMatch, PlayerSlot) {
    let mut setup = MatchSetup::duo(LinkModel::PERFECT, InProcessMatch::SEED_CHAIN);
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
    };
    // Reserved: a new player finds no slot, and the leaver takes theirs back, on a new chain.
    let (mut local, slot) = left_past_grace(rules(Leaver::Reserve, false, false));
    assert_eq!(
        local.server_inputs()[1..],
        [ServerInput::Leave {
            slot,
            reason: LeaveReason::Grace,
            becomes: AfterLeave::Reserve,
        }]
    );
    assert_eq!(controller(&local, slot), Some(Controller::Reserved));
    let late = local.add_client(2);
    for _ in 0..60 {
        local.step();
    }
    assert_eq!(local.log().take::<JoinRefused>().len(), 1);
    assert!(!playing(&local, late));
    local.mend_link(1);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, 1));
    assert!(matches!(
        local.server_inputs().last(),
        Some(ServerInput::Join { slot: joined, .. }) if *joined == slot
    ));

    // Played by a bot, which a new player takes over.
    let (mut local, slot) = left_past_grace(rules(Leaver::Bot, true, true));
    assert_eq!(controller(&local, slot), Some(Controller::Bot));
    let late = local.add_client(2);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, late));
    assert_eq!(slot_of(&local, late), slot);

    // Open, which a new player takes.
    let (mut local, slot) = left_past_grace(rules(Leaver::Open, true, false));
    assert_eq!(controller(&local, slot), Some(Controller::Open));
    let late = local.add_client(2);
    step_until(&mut local, REJOIN_STEPS, |local| playing(local, late));
    assert_eq!(slot_of(&local, late), slot);
    assert!(local.log().take::<LinkLost>().len() <= 1);
}

#[test]
fn every_client_rejoins_a_restored_server() {
    let data = Scratch::new();
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.keep_data(data.path("data"));
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
        "[[order]]\ntick = {}\nmove = [3, -2]\n",
        local.next_tick(End::Server)
    );
    for client in [0, 1] {
        local.play(client, OrderScript::parse(&script).unwrap());
    }
    step_until(&mut local, REJOIN_STEPS, |local| {
        playing(local, 0) && playing(local, 1)
    });
    let world = local.server().world();
    assert_eq!(world.resource::<TickHashes>().get()[..before.len()], before);
    let inputs = local.server_inputs();
    for slot in slots {
        assert!(inputs.contains(&ServerInput::Disconnected { slot }));
        assert!(inputs.contains(&ServerInput::Connected { slot }));
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
    drop(data);
}
