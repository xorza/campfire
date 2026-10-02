use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    round_trip(&LinkLost {
        reason: "Transport error: certificate hash mismatch".to_owned(),
    });
}
