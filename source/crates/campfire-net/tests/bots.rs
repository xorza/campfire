//! The server's bots: one of the plan's slots, which plays its script's orders and mode inputs in
//! their ticks, and the bot that takes a slot over after its player left.

use std::time::Duration;

use bevy_ecs::query::With;
use campfire_capabilities::{
    Destination, Experience, Leaver, ModeState, Owner, PlayersData, StateValue,
};
use campfire_common::PlayerSlot;
use campfire_net::internals::{End, LinkModel, LocalMatch, MatchSetup};
use campfire_net::{PlayerLink, SessionTimes, TickHashes};
use campfire_protocol::{AfterLeave, Controller, ServerInput, SessionLog};
use campfire_runner::{Runner, Session};

/// Where the server's hero of `slot` walks to.
fn destination(local: &mut LocalMatch, slot: PlayerSlot) -> Destination {
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<(&Owner, &Destination), With<Experience>>();
    let (_, destination) = heroes
        .iter(world)
        .find(|(owner, _)| owner.slot() == slot)
        .unwrap();
    *destination
}

/// The lane mode's one state field, the word picked.
fn picked(local: &LocalMatch) -> StateValue {
    local.server().world().resource::<ModeState>().get()[0].clone()
}

#[test]
fn a_server_bot_plays_its_orders_and_its_pick_in_their_ticks_and_its_log_verifies() {
    // One player in slot 0, the server's bot in slot 1: it picks "runner" in tick 5, and walks to
    // (3, 2) in tick 10.
    let mut setup = MatchSetup::SOLO;
    setup.bot = Some(
        "[[input]]\ntick = 5\nname = \"pick\"\nvalue = \"runner\"\n\
         [[order]]\ntick = 10\nmove = [3, 2]\n",
    );
    let mut local = LocalMatch::new(setup);
    local.start_match();
    let bot = PlayerSlot::new(1);
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
    // The match start ran the ticks before the first one seen, and the bot had nothing then.
    let text = |word: &str| StateValue::Text(word.to_owned());
    let first = seen[0].0;
    assert!(first < 5);
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
            .all(|input| matches!(input, ServerInput::Bot { slot, .. } if *slot == bot))
    );

    // The log replays to the server's hashes.
    let world = local.server().world();
    let mut bytes = Vec::new();
    world.resource::<Session>().log().encode(&mut bytes);
    let live = world.resource::<TickHashes>().get().to_vec();
    let decoded = SessionLog::decode(&bytes).unwrap();
    let seeds = LocalMatch::SEED_CHAIN.seeds();
    let mut replay = Runner::new(decoded.rewound(), seeds, local.packages()).unwrap();
    let mut replayed = Vec::new();
    for _ in &live {
        replay.run_tick();
        replayed.push(replay.state_hash());
    }
    assert_eq!(replayed, live);
}

#[test]
fn a_leavers_slot_under_bot_plays_the_takeover_bots_orders() {
    // Client 1 leaves after a second of grace; its slot becomes a bot's, which walks to (-3, 2)
    // 2 ticks after.
    let mut setup = MatchSetup::duo(LinkModel::PERFECT, LocalMatch::SEED_CHAIN);
    setup.rules = PlayersData {
        late_join: false,
        bot_takeover: false,
        leaver: Leaver::Bot,
    };
    setup.times = SessionTimes {
        grace: Duration::from_secs(1),
        restore_window: Duration::from_secs(1),
    };
    setup.takeover = Some("[[order]]\ntick = 2\nmove = [-3, 2]\n");
    let mut local = LocalMatch::new(setup);
    local.start_match();
    let world = local.server().world();
    let slot = world.get::<PlayerLink>(local.link(1)).unwrap().slot();
    local.cut_link(1);
    let left = |local: &LocalMatch| {
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
    assert_eq!(destination(&mut local, slot), Destination::default());
    for _ in 0..5 {
        local.step();
    }
    assert_ne!(destination(&mut local, slot), Destination::default());
    assert!(
        local
            .server_inputs()
            .iter()
            .any(|input| matches!(input, ServerInput::Bot { slot: played, .. } if *played == slot))
    );
}
