use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    round_trip(&ReceiptRefused {
        reason: "it names a head that is not the player's at its seq".to_owned(),
    });
}
