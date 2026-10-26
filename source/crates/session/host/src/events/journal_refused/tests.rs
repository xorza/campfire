use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for refusal in [
        JournalRefusal::NotLocal,
        JournalRefusal::ByMode,
        JournalRefusal::NoData,
        JournalRefusal::NoSave,
        JournalRefusal::LogFull,
        JournalRefusal::Replay,
    ] {
        round_trip(&JournalRefused { refusal });
    }
}
