//! The test lane mode, with no player input, plays 30 s to its golden record.

use std::num::NonZeroU32;
use std::path::Path;

use campfire_capabilities::ScriptFailures;
use campfire_package::ModePackages;
use campfire_runner::{FixedSession, Golden};

const LANE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);

#[test]
fn the_lane_match_plays_to_its_golden_record() {
    let packages =
        ModePackages::from_dir(Path::new(LANE)).unwrap_or_else(|error| panic!("{error}"));
    let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
    let mut golden = Golden::new(session.packages(), session.players());
    let mut fixed = session.start();
    for tick in 0..900 {
        fixed.runner_mut().run_tick();
        golden.record(fixed.runner());
        let failures = fixed.runner().world().non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
    }
    golden.check("lane");
}
