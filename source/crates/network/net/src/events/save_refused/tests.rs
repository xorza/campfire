use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for refusal in [
        SaveRefusal::NotLocal,
        SaveRefusal::ByMode,
        SaveRefusal::NoData,
        SaveRefusal::NoSave,
    ] {
        round_trip(&SaveRefused { refusal });
    }
}
