use campfire_log::internals::round_trip;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for refusal in [
        ReceiptRefusal::NotPlaying,
        ReceiptRefusal::BadSignature,
        ReceiptRefusal::Other,
        ReceiptRefusal::OtherHead,
        ReceiptRefusal::Older,
    ] {
        round_trip(&ReceiptRefused { refusal });
    }
}
