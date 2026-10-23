use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    round_trip(&LoadFailed {
        error: "No such file or directory".to_owned(),
    });
}
