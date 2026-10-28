//! The server's bots: one of the plan's slots, which plays its script's orders and mode inputs in
//! their ticks, and the bot that takes a slot over after its player left.

use std::time::Duration;

use bevy_ecs::query::With;
use campfire_common::{AfterLeave, PlayerSlot, StateHash};
use campfire_host::SessionTimes;
use campfire_host::internals::TickHashes;
use campfire_math::{Num, Vec3};
use campfire_mode::PlayersData;
use campfire_mode::internals::ModeWorldInternals;
use campfire_net::internals::SimServerWorldInternals;
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_players::PlayersWorld;
use campfire_progression::{Experience, PerkBook, PlayerPerks, PlayerPoints};
use campfire_protocol::{Controller, ServerInput};
use campfire_runner::Session;
use campfire_script::StateValue;
use campfire_stats::PlayerModifiers;
use campfire_store::internals::TestDir;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{Destination, Owner};

/// Where the server's hero of `player_slot` walks to.
fn destination(local: &mut InProcessMatch, player_slot: PlayerSlot) -> Destination {
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<(&Owner, &Destination), With<Experience>>();
    let (_, destination) = heroes
        .iter(world)
        .find(|(owner, _)| owner.player_slot() == player_slot)
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
    let setup = InProcessSetup::SOLO
        .into_builder()
        .bots(
            &["[[input]]\ntick = 5\nname = \"pick\"\nvalue = \"runner\"\n\
         [[order]]\ntick = 10\ndirective.orders.move.to = [3, 2]\n"],
        )
        .build();
    let mut local = InProcessMatch::new(setup);
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
        inputs.iter().all(
            |input| matches!(input, ServerInput::Bot { player_slot, .. } if *player_slot == bot)
        )
    );

    // The log replays to the server's hashes.
    local.assert_log_replays();
}

#[test]
fn a_leavers_slot_under_bot_plays_the_takeover_bots_orders() {
    // Client 1 leaves after a second of grace; its slot becomes a bot's, which walks to (-3, 2)
    // 2 ticks after.
    let setup = InProcessSetup::duo(LinkModel::perfect(), InProcessMatch::SEED_CHAIN)
        .into_builder()
        .rules(PlayersData {
            late_join: false,
            bot_takeover: false,
            leaver: AfterLeave::Bot,
            ..PlayersData::DEFAULT
        })
        .times(SessionTimes {
            grace: Duration::from_secs(1),
            restore_window: Duration::from_secs(1),
        })
        .takeover(Some(
            "[[order]]\ntick = 2\ndirective.orders.move.to = [-3, 2]\n",
        ))
        .build();
    let mut local = InProcessMatch::new(setup);
    local.start_match();
    let world = local.server().world();
    let player_slot = world.link_player_slot(local.link(1)).unwrap();
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
    assert_eq!(destination(&mut local, player_slot), Destination::default());
    for _ in 0..5 {
        local.step();
    }
    assert_ne!(destination(&mut local, player_slot), Destination::default());
    assert!(
        local
            .server_inputs()
            .iter()
            .any(|input| matches!(input, ServerInput::Bot { player_slot: played, .. } if *played == player_slot))
    );
}

/// The bot's seven moves of tick 10, to x = 1 to 7 m: the session takes four inputs of a slot a
/// tick, so the last three spill into tick 11.
const BURST: &str = "[[order]]\ntick = 10\ndirective.orders.move.to = [1, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [2, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [3, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [4, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [5, 0]\n[[order]]\ntick = 10\ndirective.orders.move.to = [6, 0]\n\
     [[order]]\ntick = 10\ndirective.orders.move.to = [7, 0]\n";

/// What a match of a bot ran: the bot's inputs the log holds, the state hash after each tick,
/// and where the bot's hero walks after ticks 10 and 11.
#[derive(Debug, PartialEq)]
struct Played {
    payloads: Vec<Vec<u8>>,
    hashes: Vec<StateHash>,
    walks: [Destination; 2],
}

/// The match of `setup`'s bot in slot 1 up to tick `end`, its server stopped and restored from
/// its journal after tick 10 when `stop`.
fn played(setup: InProcessSetup, stop: bool, end: u64) -> Played {
    let test_dir = TestDir::new();
    let mut local = InProcessMatch::new(setup);
    local.keep_data(test_dir.path("data"));
    local.start_match();
    let bot = PlayerSlot::new(1);
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
    Played {
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
    let setup = InProcessSetup::SOLO.into_builder().bots(&[BURST]).build();
    let whole = played(setup.clone(), false, 20);
    assert_eq!(whole.payloads.len(), 7);
    assert_ne!(whole.walks[0], whole.walks[1]);
    assert_eq!(played(setup, true, 20), whole);
}

/// The lane mode's bot `walk` in slot 1, after the player's slot 0.
fn walk() -> InProcessSetup {
    InProcessSetup::SOLO
        .into_builder()
        .mode_bots(&["walk"])
        .build()
}

#[test]
fn a_mode_bot_thinks_at_its_period_and_the_log_holds_its_orders_as_its_inputs() {
    // The lane mode's 30 Hz makes the bot's 100 ms 3 ticks; slot 1 thinks a tick after slot 0
    // would, before ticks 1, 4, 7 and 10. Each think walks its hero to (3, 2), its param `to`,
    // one input each.
    let mut local = InProcessMatch::new(walk());
    local.start_match();
    let bot = PlayerSlot::new(1);
    let session = local.server().world().resource::<Session>();
    assert_eq!(session.log().controller(bot), Some(Controller::Bot));
    let mut seen = Vec::new();
    while local.next_tick(End::Server) < 12 {
        let tick = local.next_tick(End::Server);
        local.step();
        if local.next_tick(End::Server) > tick {
            seen.push((tick, local.server_inputs().len()));
        }
    }
    let thinks = [1, 4, 7, 10];
    let expected: Vec<_> = (2..12)
        .map(|tick| (tick, thinks.iter().filter(|&&at| at <= tick).count()))
        .collect();
    assert_eq!(seen, expected);
    assert!(
        local.server_inputs().iter().all(
            |input| matches!(input, ServerInput::Bot { player_slot, .. } if *player_slot == bot)
        )
    );
    let to = |value| Num::from_int(value).unwrap();
    let walk = Destination::to(Some(Vec3::new(to(3), Num::ZERO, to(2))));
    assert_eq!(destination(&mut local, bot), walk);
    local.assert_log_replays();
}

#[test]
fn a_restored_server_plays_a_mode_bot_on_from_the_world_it_finds() {
    // Stopped after tick 10, the restored server builds the bot again, which thinks before tick
    // 13 as the bot of the whole match does: both log the same inputs and hash alike. In 20
    // ticks it thinks before ticks 1, 4, 7, 10, 13, 16 and 19.
    let whole = played(walk(), false, 20);
    assert_eq!(whole.payloads.len(), 7);
    assert_eq!(played(walk(), true, 20), whole);
}

#[test]
fn a_mode_bot_learns_a_perk_by_an_order_of_no_units_and_its_log_learns_the_same() {
    // The lane mode of a player track `rank` whose level 1 gives a point, and the perk `drill` of
    // cost 1: the bot `walk` learns it in its first think, before tick 1, as it holds a point and
    // lacks it, by an order of no units beside its walk; its think before tick 4 walks alone.
    const RANK: &str = "[tracks.rank]\nholder = \"player\"\nlevels = [50]\n\n\
                        [perks.drill]\ncost = 1\nmodifier = \"drill\"\n\n\
                        [modifiers.drill]\nstats = { move_speed = { cut = \"0.1\" } }\n";
    const LEARN: &str = "fn on_bot_think(ctx) {\n    \
                         if ctx.player.perk_points > 0 && !ctx.player.has_perk(\"drill\") {\n        \
                         ctx.order([], #{ progression: #{ learn_perk: #{ perk: \"drill\" } } });\n    \
                         }\n";
    let mut tree = PackageTree::of(TestMode::Lane);
    tree.edit("modes/lane/data/mode.toml", Edit::Append(RANK))
        .edit(
            "modes/lane/scripts/walk_bot.rhai",
            Edit::Replace("fn on_bot_think(ctx) {\n", LEARN),
        );
    let mut local = InProcessMatch::of_mode(walk(), tree.packages());
    local.start_match();
    let bot = PlayerSlot::new(1);
    let mut seen = Vec::new();
    while local.next_tick(End::Server) < 6 {
        let tick = local.next_tick(End::Server);
        local.step();
        if local.next_tick(End::Server) > tick {
            let world = local.server().world();
            let player = world.player_entity(bot).unwrap();
            let book = world.resource::<PerkBook>();
            let drill = book.names().named("drill").unwrap();
            let perks = world.get::<PlayerPerks>(player).unwrap();
            let points = world.get::<PlayerPoints>(player).unwrap().get();
            let modifiers = world.get::<PlayerModifiers>(player).unwrap();
            assert_eq!(
                perks.holds(drill),
                modifiers.holds(book.modifier(drill)),
                "tick {tick}"
            );
            seen.push((
                tick,
                perks.holds(drill),
                points,
                local.server_inputs().len(),
            ));
        }
    }
    // The match starts through tick 1, whose think's two orders spent the point; the think
    // before tick 4 gives its walk alone.
    let expected: Vec<_> = (2..6)
        .map(|tick| (tick, true, 0, if tick < 4 { 2 } else { 3 }))
        .collect();
    assert_eq!(seen, expected);
    local.assert_log_replays();
}
