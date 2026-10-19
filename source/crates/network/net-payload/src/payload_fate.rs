use crate::payload_token::PayloadToken;

/// What became of a payload the packet session sent: its packet reached the peer and was accepted,
/// or it was lost. The packet session reports each payload's fate once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadFate {
    Delivered(PayloadToken),
    Lost(PayloadToken),
}
