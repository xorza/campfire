//! A mode that runs a rules package's script as its own, with no capability.

use std::num::NonZeroU32;

use campfire_harness::FixedSession;
use campfire_package_disk::internals::TestMode;
use campfire_protocol::Outcome;
use campfire_runner::{MatchBuild, SessionWorld};
use campfire_sim::SimWorld;
use campfire_sim::internals::SimWorldInternals;

#[test]
fn a_mode_runs_the_script_its_rules_package_holds() {
    let packages = TestMode::Ruled.packages();
    // The rules package's script ends the match as it starts, its first team the winner.
    let mut target = MatchBuild::test_target(&packages, 1);
    let mut fixed = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 1).start();
    fixed.game_mut().run_tick_alone();
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });
    // A match of no capability restores, its checks reading no book of one.
    let mut snapshot = Vec::new();
    fixed.game().snapshot(&mut snapshot);
    target.restore_in_place(&snapshot).unwrap();
}
