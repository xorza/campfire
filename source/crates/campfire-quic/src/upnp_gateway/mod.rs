use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use igd_next::{
    AddAnyPortError, AddPortError, GatewayIpVersion, PortMappingProtocol, SearchOptions,
};

use crate::error::MapError;
use crate::gateway::{MappedBy, Mapping};

/// The SSDP address a UPnP router answers a search on.
pub(crate) const SSDP: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 1900);
/// What a mapping is for, in the router's list of them.
const DESCRIPTION: &str = "campfire";

/// A UPnP-IGD router, through `igd-next`'s blocking API, and the address of this host on its side
/// (design 15, D12).
#[derive(Debug)]
pub(crate) struct UpnpGateway {
    router: igd_next::Gateway,
    local: Ipv4Addr,
}

impl UpnpGateway {
    /// The first router on IPv4 that answers a search sent to `ssdp` within `timeout`. A search
    /// sent to one host, not to SSDP's group, trusts that host's description at its address,
    /// loopback too, as it asked that host alone.
    pub(crate) fn search(ssdp: SocketAddr, timeout: Duration) -> Result<UpnpGateway, MapError> {
        let mut options = SearchOptions::default();
        options.broadcast_address = ssdp;
        options.allowed_gateway_ips = vec![ssdp.ip()];
        options.timeout = Some(timeout);
        options.single_search_timeout = Some(timeout);
        options.gateway_ip_version = GatewayIpVersion::V4;
        let router = igd_next::search_gateway(options).map_err(MapError::UpnpSearch)?;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(MapError::Io)?;
        socket.connect(router.addr).map_err(MapError::Io)?;
        let IpAddr::V4(local) = socket.local_addr().map_err(MapError::Io)?.ip() else {
            unreachable!("a router on IPv4 is reached from an IPv4 address");
        };
        Ok(UpnpGateway { router, local })
    }

    /// Maps `internal_port` for `lifetime`: anew at the same port outside, or at any other one
    /// when another host holds that; or `renewed` again at its external port. A router that takes
    /// only a lease that never ends gets one, which the mapping still renews within `lifetime`,
    /// as a router that restarted forgets it.
    pub(crate) fn map(
        &self,
        internal_port: u16,
        renewed: Option<&Mapping>,
        lifetime: Duration,
    ) -> Result<Mapping, MapError> {
        let local = SocketAddr::from((self.local, internal_port));
        let mut lease = u32::try_from(lifetime.as_secs())
            .expect("a mapping's lifetime fits UPnP's 32 bits of seconds");
        let external_port = renewed.map_or(internal_port, |mapping| mapping.external.port());
        let added = match self.add(external_port, local, lease) {
            Err(AddPortError::OnlyPermanentLeasesSupported) => {
                lease = 0;
                self.add(external_port, local, lease)
            }
            added => added,
        };
        let port = match added {
            Ok(()) => external_port,
            Err(AddPortError::PortInUse) if renewed.is_none() => match self.add_any(local, lease) {
                Err(AddAnyPortError::OnlyPermanentLeasesSupported) => self.add_any(local, 0),
                added => added,
            }
            .map_err(MapError::UpnpAddAny)?,
            Err(error) => return Err(MapError::UpnpAdd(error)),
        };
        let address = match self
            .router
            .get_external_ip()
            .map_err(MapError::UpnpExternal)?
        {
            IpAddr::V4(address) => address,
            address @ IpAddr::V6(_) => return Err(MapError::ExternalNotIpv4(address)),
        };
        Ok(Mapping {
            external: SocketAddrV4::new(address, port),
            lifetime,
            by: MappedBy::Upnp,
        })
    }

    /// Deletes `mapping`, which this router granted. `igd-next` bounds the request at 10 s.
    pub(crate) fn delete(&self, mapping: &Mapping) -> Result<(), MapError> {
        debug_assert_eq!(mapping.by, MappedBy::Upnp, "a mapping of this router");
        self.router
            .remove_port(PortMappingProtocol::UDP, mapping.external.port())
            .map_err(MapError::UpnpRemove)
    }

    fn add_any(&self, local: SocketAddr, lease: u32) -> Result<u16, AddAnyPortError> {
        self.router
            .add_any_port(PortMappingProtocol::UDP, local, lease, DESCRIPTION)
    }

    fn add(&self, external_port: u16, local: SocketAddr, lease: u32) -> Result<(), AddPortError> {
        self.router.add_port(
            PortMappingProtocol::UDP,
            external_port,
            local,
            lease,
            DESCRIPTION,
        )
    }
}

#[cfg(test)]
pub(crate) mod fake_router;

#[cfg(test)]
mod tests;
