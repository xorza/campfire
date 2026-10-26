use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_as_it_logs() {
    round_trip(&ProtocolDiffers {
        link: 4_294_967_303,
        differing: "channel registry, message registry".to_owned(),
    });
}
