use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use crate::nat_pmp_map::NatPmpRefusal;
use crate::pcp_map::PcpRefusal;

/// An endpoint's socket did not open on `address`: the port is in use, or the OS refused it.
#[derive(Debug, thiserror::Error)]
#[error("a QUIC socket does not open on {address}")]
pub struct BindError {
    pub address: SocketAddr,
    #[source]
    pub error: io::Error,
}

/// A gateway's PCP or NAT-PMP reply that does not read: the reply it should be, by its version, its
/// opcode and its length, or a result code the protocol does not define.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReplyError {
    #[error("a reply of {len} bytes, where it takes {needed}")]
    TooShort { len: usize, needed: usize },
    #[error("a reply of version {0}")]
    Version(u8),
    #[error("a reply with opcode {0}, which answers another request")]
    Opcode(u8),
    #[error("a reply with result code {0}, which the protocol does not define")]
    Result(u16),
    #[error("a reply that maps protocol {0}, not UDP")]
    Protocol(u8),
    #[error("a reply whose external address is no IPv4 address")]
    NotIpv4,
}

/// A gateway did not map a port: it never answered, no server listens on its port, it refused, or
/// the client's socket failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MapError {
    #[error("the OS names no IPv4 gateway: {0}")]
    NoGateway(String),
    #[error("the gateway did not answer")]
    NoAnswer,
    #[error("no PCP or NAT-PMP server listens on the gateway's port")]
    NoServer,
    #[error("the gateway refused by PCP, {refusal:?}, for {lasts:?}")]
    PcpRefused {
        refusal: PcpRefusal,
        lasts: Duration,
    },
    #[error("the gateway speaks NAT-PMP alone now, which holds no PCP mapping")]
    NatPmpOnly,
    #[error("the gateway refused by NAT-PMP, {0:?}")]
    NatPmpRefused(NatPmpRefusal),
    #[error("no UPnP router answered")]
    UpnpSearch(#[source] igd_next::SearchError),
    #[error("the UPnP router did not map the port")]
    UpnpAdd(#[source] igd_next::AddPortError),
    #[error("the UPnP router mapped no other port")]
    UpnpAddAny(#[source] igd_next::AddAnyPortError),
    #[error("the UPnP router did not give its external address")]
    UpnpExternal(#[source] igd_next::GetExternalIpError),
    #[error("the UPnP router did not delete the mapping")]
    UpnpRemove(#[source] igd_next::RemovePortError),
    #[error("the router's external address {0} is no IPv4 address")]
    ExternalNotIpv4(IpAddr),
    #[error("the socket to the gateway failed")]
    Io(#[source] io::Error),
}
