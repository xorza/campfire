use std::num::{NonZeroU8, NonZeroU32};

use bevy_state::state::{NextState, State};
use campfire_bots::{BotNames, BotScript, OrderScript};
use campfire_common::{AfterLeave, ExitStatus, PlayerSlot, Tick};
use campfire_harness::{FixedMatch, FixedSession};
use campfire_host::SessionJournal;
use campfire_host::{BotPlay, PlayerSlotBot};
use campfire_mode::{HostPicks, PlayersData, SaveBy, SavesData};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{
    Controller, InputRules, Journal, LeaveReason, PlayerSlotPlan, PlayerSlotStart, SeedChain,
    ServerInput, SessionHeader, SessionLog,
};
use campfire_runner::{HostChoices, SessionRules};
use campfire_store::AppendWriter;
use campfire_test_packages::{PackageTree, TestMode};
use campfire_threads::ThreadName;

use super::*;
use crate::sim_server::sim_server_world::internals::SimServerWorldInternals;

#[test]
fn a_failed_journal_ends_the_server_with_its_exit_code() {
    // A session of the test lane mode with one open player slot, so its header needs no player; the
    // log's header is its journal's first record, whose sync fails.
    let packages = TestMode::Lane.packages();
    let key = TestKey::of(8);
    let rules = SessionRules::of(&packages);
    let terms = rules
        .terms(HostChoices {
            server_key: key.x_only_public_key().0,
            seed_commitment: SeedChain::new([9; 32], NonZeroU32::MIN).commitment(),
            tick_hz: packages.manifest().tick_hz.default(),
            inputs: InputRules::LAN,
            player_slots: vec![PlayerSlotPlan::Open],
            visibility: rules.visibility().settings(),
            picks: &HostPicks::defaults(packages.data(), 1),
        })
        .unwrap()
        .terms;
    let header = SessionHeader {
        terms,
        player_slots: vec![PlayerSlotStart::Open],
    };
    let log = SessionLog::new(header).unwrap();
    let journal = SessionJournal::of(AppendWriter::failing_sync(ThreadName::fixture("journal")));
    let mut world = World::new();
    world.init_resource::<WorkerFaults>();
    world.insert_resource(State::new(ServerPhase::Lobby));
    world.init_resource::<NextState<ServerPhase>>();
    world.insert_resource(JournalWatch(journal.watch()));
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    let journal = Journal::start(&log, Box::new(journal));
    // Closed, as a session that ends closes it, the log's journal waits for its writer, which
    // stopped at the failed sync, so the failure is there for the exit.
    drop(journal);
    let code = NonZeroU8::new(ExitStatus::Storage.code()).unwrap();
    assert_eq!(
        world.exit_due(StopRequest::NotAsked),
        Some(AppExit::Error(code))
    );
    // The failure is given once; before the match, only a stop exits.
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    assert_eq!(world.exit_due(StopRequest::Asked), Some(AppExit::Success));
}

#[test]
fn a_plan_bot_plays_its_script_until_its_slot_changes_hands() {
    // Slot 1 is the plan's bot; player 1 takes it over and leaves it to a bot again. A restore
    // from there finds the slot changed hands: it plays the takeover script, which
    // `follow_controllers` gives it, and the plan's no more.
    let rules = PlayersData {
        late_join: true,
        bot_takeover: true,
        leaver: AfterLeave::Bot,
        ..PlayersData::DEFAULT
    };
    let saves = SavesData {
        by: SaveBy::Player,
        autosave_ms: None,
    };
    let packages = PackageTree::of(TestMode::Lane)
        .players_and_saves(&rules, &saves)
        .packages();
    let script =
        OrderScript::parse("[[order]]\ntick = 2\ndirective.orders.move.to = [1, 0]\n").unwrap();
    let script = BotScript::new(&script, &BotNames::new(packages.load_books())).unwrap();
    let plan = [PlayerSlotPlan::Player, PlayerSlotPlan::Bot];
    let mut fixed = FixedSession::planned(packages, plan).start();
    let player_slot = PlayerSlot::new(1);
    let planned = |fixed: &FixedMatch| {
        let slots = vec![PlayerSlotBot {
            player_slot,
            play: BotPlay::Orders(script.clone()),
        }];
        fixed.game().planned_bots(slots, Tick::new(0))
    };
    assert_eq!(planned(&fixed), [player_slot]);
    fixed.join(1).unwrap();
    fixed
        .serve(ServerInput::Leave {
            player_slot,
            reason: LeaveReason::Asked,
            becomes: AfterLeave::Bot,
        })
        .unwrap();
    assert_eq!(
        fixed.game().session().log().controller(player_slot),
        Some(Controller::Bot)
    );
    assert_eq!(planned(&fixed), []);
}
