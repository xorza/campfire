use std::io;
use std::net::IpAddr;
use std::time::Duration;

use crate::port_map::nat_pmp_map::NatPmpRefusal;
use crate::port_map::pcp_map::PcpRefusal;

/// A gateway did not map a port: it never answered, no server listens on its port, it refused,
/// the client's socket failed, or the map stopped.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MapError {
    #[error("the OS names no default gateway: {0}")]
    NoGateway(String),
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
    #[error("no UPnP router answered")]
    UpnpSearch(#[source] igd_next::SearchError),
    #[error("the announced UPnP router's document did not come")]
    UpnpFetch(#[source] attohttpc::Error),
    #[error("the announced UPnP router's document did not read in full, within the map's bound")]
    UpnpRead(#[source] io::Error),
    #[error("the announced UPnP router's description names no WAN connection at its address")]
    UpnpDescription,
    #[error("the announced UPnP router's service schema does not read")]
    UpnpSchema,
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
    /// The map stopped before the router answered.
    #[error("the map stopped")]
    Stopped,
}
