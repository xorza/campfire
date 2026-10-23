use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    round_trip(&BotThinkFailed {
        player_slot: PlayerSlot::new(1),
        error: "value is no unit".to_owned(),
    });
}
