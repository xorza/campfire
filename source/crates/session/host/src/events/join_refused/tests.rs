use campfire_log::internals::round_trip;
use campfire_protocol::{DelegationError, EventError};

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for error in [
        JoinError::Full,
        JoinError::Delegation(DelegationError::NotEvent(EventError::Malformed)),
    ] {
        round_trip(&JoinRefused { link: 14, error });
    }
}
