use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    round_trip(&JoinRefused {
        link: "14v1".to_owned(),
        error: "every slot is taken".to_owned(),
    });
}
