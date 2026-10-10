//! A mode that runs a rules package's script as its own.

use std::num::NonZeroU32;

use campfire_common::MapName;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::Outcome;
use campfire_runner::internals::FixedSession;

#[test]
fn a_mode_runs_the_script_its_rules_package_holds() {
    let packages = ModePackages::from_dir(
        &PackageDir::workspace("test/modes/ruled"),
        &MapName::new("stand").unwrap(),
    )
    .unwrap();
    // The rules package's script ends the match as it starts, its first team the winner.
    let mut fixed = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 1).start();
    fixed.runner_mut().run_tick();
    assert_eq!(fixed.end().outcome, Outcome::Won { team: 0 });
}
