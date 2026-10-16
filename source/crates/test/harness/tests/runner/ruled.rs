//! A mode that runs a rules package's script as its own, with no capability.

use std::num::NonZeroU32;

use campfire_capabilities::Match;
use campfire_harness::FixedSession;
use campfire_package::ModePackages;
use campfire_package_disk::internals::TestMode;
use campfire_protocol::{Outcome, SessionLog, SessionResult};
use campfire_runner::{MatchBuild, Replay, ReplayError, ResultMismatch, SessionWorld};
use campfire_sim::SimWorld;
use campfire_sim::internals::SimWorldInternals;

#[test]
fn a_mode_runs_the_script_its_rules_package_holds() {
    let packages = TestMode::Ruled.packages();
    // The rules package's script ends the match as it starts, its first team the winner.
    let mut target = MatchBuild::test_target(&packages, 1);
    let mut fixed =
        FixedSession::new(TestMode::Ruled.packages(), NonZeroU32::new(30).unwrap(), 1).start();
    fixed.game_mut().run_tick_alone();
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });
    // A match of no capability restores, its checks reading no book of one.
    let mut snapshot = Vec::new();
    fixed.game().snapshot(&mut snapshot);
    target.restore_in_place(&snapshot).unwrap();
    assert_eq!(replayed(fixed.game(), &packages), Ok(()));

    // The same match with its result replaced by an aborted one: a host may not abort a match
    // the mode ended.
    let mut fixed =
        FixedSession::new(TestMode::Ruled.packages(), NonZeroU32::new(30).unwrap(), 1).start();
    fixed.game_mut().run_tick_alone();
    let game = fixed.game_mut();
    let aborted = SessionResult {
        outcome: Outcome::Aborted,
        ..game.session_result()
    };
    game.session_mut()
        .record_result(aborted, &FixedSession::server_signer())
        .unwrap();
    game.session_mut().reveal_seed();
    let mismatch = ResultMismatch::Outcome {
        logged: Outcome::Aborted,
        ended: Outcome::Won { team: 0 },
    };
    assert_eq!(
        replayed(fixed.game(), &packages),
        Err(ReplayError::Result(mismatch))
    );
}

/// The replay of `game`'s published log on `packages`, to its end.
fn replayed(game: &Match, packages: &ModePackages) -> Result<(), ReplayError> {
    let mut bytes = Vec::new();
    game.session().log().encode(&mut bytes);
    Replay::of(SessionLog::decode(&bytes).unwrap(), packages)
        .unwrap()
        .run()
}
