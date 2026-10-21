//! The server's bots: one of the plan's slots, which plays its script's orders and mode inputs in
//! their ticks, and the bot that takes a slot over after its player left.

use std::time::Duration;

use bevy_ecs::query::With;
use campfire_common::{AfterLeave, Seat, StateHash};
use campfire_mode::{ModeWorld, PlayersData};
use campfire_net::SessionTimes;
use campfire_net::internals::{SimServerWorldInternals, TickHashes};
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_progression::Experience;
use campfire_protocol::{Controller, ServerInput};
use campfire_runner::Session;
use campfire_script::StateValue;
use campfire_store::internals::TestDir;
use campfire_units::{Destination, Owner};

/// Where the server's hero of `seat` walks to.
fn destination(local: &mut InProcessMatch, seat: Seat) -> Destination {
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<(&Owner, &Destination), With<Experience>>();
    let (_, destination) = heroes
        .iter(world)
        .find(|(owner, _)| owner.seat() == seat)
        .unwrap();
    *destination
}

/// The lane mode's one state field, the word picked.
fn picked(local: &InProcessMatch) -> StateValue {
    local.server().world().mode_state()[0].clone()
}

#[test]
fn a_server_bot_plays_its_orders_and_its_pick_in_their_ticks_and_its_log_verifies() {
    // One player in slot 0, the server's bot in slot 1: it picks "runner" in tick 5, and walks to
    // (3, 2) in tick 10.
    let mut setup = InProcessSetup::SOLO;
    setup.bots = &["[[input]]\ntick = 5\nname = \"pick\"\nvalue = \"runner\"\n\
         [[order]]\ntick = 10\ndirective.orders.move.to = [3, 2]\n"];
    let mut local = InProcessMatch::new(setup);
    local.start_match();
    let bot = Seat::new(1);
    let session = local.server().world().resource::<Session>();
    assert_eq!(session.log().controller(bot), Some(Controller::Bot));
    let mut seen = Vec::new();
    while local.next_tick(End::Server) < 12 {
        let tick = local.next_tick(End::Server);
        local.step();
        if local.next_tick(End::Server) > tick {
            let walking = destination(&mut local, bot) != Destination::default();
            seen.push((tick, picked(&local), walking));
        }
    }
    // The match start ran the ticks 0 and 1, before the first one seen, and the bot had nothing
    // then.
    let text = |word: &str| StateValue::Text(word.into());
    let first = seen[0].0;
    assert_eq!(first, 2);
    let expected: Vec<_> = (first..12)
        .map(|tick| {
            (
                tick,
                text(if tick >= 5 { "runner" } else { "" }),
                tick >= 10,
            )
        })
        .collect();
    assert_eq!(seen, expected);
    // The server logged the two as the bot's inputs, signed by its key.
    let inputs = local.server_inputs();
    assert_eq!(inputs.len(), 2);
    assert!(
        inputs
            .iter()
            .all(|input| matches!(input, ServerInput::Bot { seat, .. } if *seat == bot))
    );

    // The log replays to the server's hashes.
    local.assert_log_replays();
}

#[test]
fn a_leavers_slot_under_bot_plays_the_takeover_bots_orders() {
    // Client 1 leaves after a second of grace; its slot becomes a bot's, which walks to (-3, 2)
    // 2 ticks after.
    let mut setup = InProcessSetup::duo(LinkModel::perfect(), InProcessMatch::SEED_CHAIN);
    setup.rules = PlayersData {
        late_join: false,
        bot_takeover: false,
        leaver: AfterLeave::Bot,
        ..PlayersData::DEFAULT
    };
    setup.times = SessionTimes {
        grace: Duration::from_secs(1),
        restore_window: Duration::from_secs(1),
    };
    setup.takeover = Some("[[order]]\ntick = 2\ndirective.orders.move.to = [-3, 2]\n");
    let mut local = InProcessMatch::new(setup);
    local.start_match();
    let world = local.server().world();
    let seat = world.link_seat(local.link(1)).unwrap();
    local.cut_link(1);
    let left = |local: &InProcessMatch| {
        local.server_inputs().iter().any(|input| {
            matches!(
                input,
                ServerInput::Leave {
                    becomes: AfterLeave::Bot,
                    ..
                }
            )
        })
    };
    for _ in 0..40 {
        if left(&local) {
            break;
        }
        local.step();
    }
    assert!(left(&local));
    assert_eq!(destination(&mut local, seat), Destination::default());
    for _ in 0..5 {
        local.step();
    }
    assert_ne!(destination(&mut local, seat), Destination::default());
    assert!(
        local
            .server_inputs()
            .iter()
            .any(|input| matches!(input, ServerInput::Bot { seat: played, .. } if *played == seat))
    );
}

/// The bot's seven moves of tick 10, to x = 1 to 7 m: the session takes four inputs of a slot a
/// tick, so the last three spill into tick 11.
const BURST: &str = "[[order]]\ntick = 10\ndirective.orders.move.to = [1, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [2, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [3, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [4, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [5, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [6, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [7, 0]\n";

/// What a match of the bot of `BURST` ran: the bot's inputs the log holds, the state hash after
/// each tick, and where the bot's hero walks after ticks 10 and 11.
#[derive(Debug, PartialEq)]
struct Burst {
    payloads: Vec<Vec<u8>>,
    hashes: Vec<StateHash>,
    walks: [Destination; 2],
}

/// The match of the bot of `BURST` up to tick `end`, its server stopped and restored from its
/// journal after tick 10 when `stop`.
fn burst(stop: bool, end: u64) -> Burst {
    let test_dir = TestDir::new();
    let mut setup = InProcessSetup::SOLO;
    setup.bots = &[BURST];
    let mut local = InProcessMatch::new(setup);
    local.keep_data(test_dir.path("data"));
    local.start_match();
    let bot = Seat::new(1);
    let (mut walks, mut restarted) = (Vec::new(), false);
    while local.next_tick(End::Server) < end {
        let tick = local.next_tick(End::Server);
        local.step();
        if local.next_tick(End::Server) > tick && (tick == 10 || tick == 11) {
            walks.push(destination(&mut local, bot));
        }
        if stop && tick == 10 && local.next_tick(End::Server) == 11 {
            local.restart_server();
            restarted = true;
        }
    }
    assert_eq!(restarted, stop);
    let payloads = local
        .server_inputs()
        .into_iter()
        .filter_map(|input| match input {
            ServerInput::Bot { payload, .. } => Some(payload.to_vec()),
            _ => None,
        })
        .collect();
    let hashes = local.server().world().resource::<TickHashes>().get()
        [..usize::try_from(end).unwrap()]
        .to_vec();
    Burst {
        payloads,
        hashes,
        walks: walks.try_into().unwrap(),
    }
}

#[test]
fn a_restored_server_applies_the_bot_inputs_that_spilled_past_its_stop() {
    // The log takes all seven before tick 10, and the three past its four apply in tick 11, from
    // the journal when the server stopped after tick 10: both matches log the same inputs, walk
    // the bot to 4 m after tick 10 and to 7 m after tick 11, and hash alike through tick 19.
    let whole = burst(false, 20);
    assert_eq!(whole.payloads.len(), 7);
    assert_ne!(whole.walks[0], whole.walks[1]);
    assert_eq!(burst(true, 20), whole);
}
