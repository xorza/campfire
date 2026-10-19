use campfire_log::internals::round_trip;
use campfire_net_payload::AppCode;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for cause in [
        LinkLossCause::ServerClosed(CloseCode::Left),
        LinkLossCause::ClientClosed(CloseCode::ChainRewritten),
        LinkLossCause::Link(UnlinkReason::WrongTlsKey),
        LinkLossCause::Link(UnlinkReason::ClosedByPeer(AppCode::new(99))),
        LinkLossCause::Link(UnlinkReason::Transport { code: 0x131 }),
        LinkLossCause::RetriesEnded,
    ] {
        round_trip(&LinkLost { cause });
    }
}

#[test]
fn a_link_end_names_campfires_codes_and_keeps_any_other() {
    let of = LinkLossCause::of;
    let left = CloseCode::Left.app();
    assert_eq!(
        of(UnlinkReason::ClosedByPeer(left)),
        LinkLossCause::ServerClosed(CloseCode::Left)
    );
    assert_eq!(
        of(UnlinkReason::Closed(left)),
        LinkLossCause::ClientClosed(CloseCode::Left)
    );
    let unknown = AppCode::new(99);
    assert_eq!(
        of(UnlinkReason::ClosedByPeer(unknown)),
        LinkLossCause::Link(UnlinkReason::ClosedByPeer(unknown))
    );
    assert_eq!(
        of(UnlinkReason::TimedOut),
        LinkLossCause::Link(UnlinkReason::TimedOut)
    );
}
