use std::io;
use std::net::IpAddr;
use std::time::Duration;

use crate::nat_pmp_map::NatPmpRefusal;
use crate::pcp_map::PcpRefusal;

/// A gateway did not map a port: it never answered, no server listens on its port, it refused,
/// the client's socket failed, or the map stopped.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MapError {
    #[error("the OS names no default gateway")]
    NoGateway,
    #[error("the default gateway has no IPv4 address")]
    NoIpv4Gateway,
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
    #[error("no UPnP router answered the search")]
    NoUpnpRouter,
    #[error("the UPnP router's document did not come in full, within the map's bound")]
    UpnpFetch(#[from] ureq::Error),
    #[error("the UPnP router's description names no WAN connection at its address")]
    UpnpDescription,
    #[error("the UPnP router's service schema does not read")]
    UpnpSchema,
    #[error("the UPnP router did not map the port")]
    UpnpAdd(#[source] SoapError),
    #[error("the UPnP router mapped no other port")]
    UpnpAddAny(#[source] SoapError),
    #[error("the UPnP router did not give its external address")]
    UpnpExternal(#[source] SoapError),
    #[error("the UPnP router did not delete the mapping")]
    UpnpRemove(#[source] SoapError),
    #[error("the router's external address {0} is no IPv4 address")]
    ExternalNotIpv4(IpAddr),
    #[error("the socket to the gateway failed")]
    Io(#[source] io::Error),
    /// The map stopped before the router answered.
    #[error("the map stopped")]
    Stopped,
}

/// A SOAP action that a UPnP router did not take: the request or its reply failed, the router's
/// service lacks the action, the reply is no answer to it, or the router refused it.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SoapError {
    #[error("the request did not reach the router, or its reply did not come in full")]
    Http(#[from] ureq::Error),
    #[error("the router's service offers no such action")]
    Unsupported,
    #[error("the router's reply is no SOAP answer to the action")]
    Reply,
    #[error("the router refused, with UPnP error {0}")]
    Refused(u16),
}
