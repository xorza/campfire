use campfire_log::internals::round_trip;
use campfire_quic::AppCode;

use super::*;

#[test]
fn the_event_reads_back_what_it_logs() {
    for cause in [
        LinkLossCause::ServerClosed(CloseCode::Left),
        LinkLossCause::ClientClosed(CloseCode::ChainRewritten),
        LinkLossCause::Link(LinkEnd::WrongServerKey),
        LinkLossCause::Link(LinkEnd::ClosedByPeer(AppCode::new(99))),
        LinkLossCause::Link(LinkEnd::Transport { code: 0x131 }),
        LinkLossCause::InProcess,
        LinkLossCause::RetriesEnded,
    ] {
        round_trip(&LinkLost { cause });
    }
}

#[test]
fn a_link_end_names_campfires_codes_and_keeps_any_other() {
    let of = |end| LinkLossCause::of(Some(&end));
    let left = CloseCode::Left.app();
    assert_eq!(
        of(LinkEnd::ClosedByPeer(left)),
        LinkLossCause::ServerClosed(CloseCode::Left)
    );
    assert_eq!(
        of(LinkEnd::Closed(left)),
        LinkLossCause::ClientClosed(CloseCode::Left)
    );
    let unknown = AppCode::new(99);
    assert_eq!(
        of(LinkEnd::ClosedByPeer(unknown)),
        LinkLossCause::Link(LinkEnd::ClosedByPeer(unknown))
    );
    assert_eq!(
        of(LinkEnd::TimedOut),
        LinkLossCause::Link(LinkEnd::TimedOut)
    );
    assert_eq!(LinkLossCause::of(None), LinkLossCause::InProcess);
}
