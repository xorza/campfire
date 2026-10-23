use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

/// A port a gateway maps: its port outside, its router's address outside once read, how long
/// until it is renewed, and by which protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mapping {
    pub(crate) external_port: u16,
    /// The router's address outside; none for a grant whose address did not read, which is
    /// still the router's to delete.
    pub(crate) address: Option<Ipv4Addr>,
    /// The lifetime the gateway granted; for a UPnP lease that never ends, the one asked for.
    pub(crate) lifetime: Duration,
    pub(crate) by: MappedBy,
}

/// What a map asks of a gateway or a router: `internal_port` for `lifetime`, anew, or `renewed`
/// again at its port outside.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MapAsk<'a> {
    pub(crate) internal_port: u16,
    /// What the mapping is for, in a UPnP router's list of them.
    pub(crate) name: &'static str,
    pub(crate) renewed: Option<&'a Mapping>,
    pub(crate) lifetime: Duration,
}

/// The protocol a mapping was granted by, which its renewal and deletion speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MappedBy {
    Pcp,
    NatPmp,
    Upnp,
}

impl Mapping {
    /// Where a player outside reaches the port, once its address read.
    pub(crate) fn external(&self) -> Option<SocketAddrV4> {
        Some(SocketAddrV4::new(self.address?, self.external_port))
    }
}
