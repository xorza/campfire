/// The binding of the TLS packet session a link's packet session came from: RFC 9266's
/// `tls-exporter` value, which both ends read alike, so an app's login signs it and a relayed login
/// fails. A plain packet session has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelBinding(pub [u8; 32]);
