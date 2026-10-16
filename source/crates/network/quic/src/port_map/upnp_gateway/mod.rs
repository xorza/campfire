use std::io::{self, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use igd_next::{
    AddAnyPortError, AddPortError, GatewayIpVersion, PortMappingProtocol, SearchOptions,
};

use crate::port_map::control_schema::ControlSchema;
use crate::port_map::error::MapError;
use crate::port_map::facing_socket::FacingSocket;
use crate::port_map::gateway::{Ask, MappedBy, Mapping};
use crate::port_map::router_description::RouterDescription;
use crate::port_map::router_url::RouterUrl;
use crate::port_map::ssdp_announcement::SsdpAnnouncement;

/// The SSDP address a UPnP router answers a search on.
pub(crate) const SSDP: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 1900);
/// What a mapping is for, in the router's list of them.
const DESCRIPTION: &str = "campfire";
/// The most bytes of a router's document the map reads, as `igd-next` bounds a search's.
pub(crate) const MAX_DOCUMENT: usize = 1 << 20;

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
    pub(crate) fn search(ssdp: SocketAddr, timeout: Duration) -> Result<Self, MapError> {
        let mut options = SearchOptions::default();
        options.broadcast_address = ssdp;
        options.allowed_gateway_ips = vec![ssdp.ip()];
        options.timeout = Some(timeout);
        options.single_search_timeout = Some(timeout);
        options.gateway_ip_version = GatewayIpVersion::V4;
        let router = igd_next::search_gateway(options).map_err(MapError::UpnpSearch)?;
        Self::of(router)
    }

    /// The router of `announcement`, built as `igd-next` builds one from a search's answer: its
    /// description and its service schema read at its location, each within `timeout`, every URL
    /// at its address (design 15, D12); the map's stop ends it between the two reads.
    pub(crate) fn announced(
        announcement: &SsdpAnnouncement,
        timeout: Duration,
        stop: &AtomicBool,
    ) -> Result<Self, MapError> {
        let location = announcement.location();
        let description = Self::fetch(location, timeout)?;
        let description =
            RouterDescription::parse(&description, location).ok_or(MapError::UpnpDescription)?;
        running(stop)?;
        let schema = Self::fetch(description.schema(), timeout)?;
        let schema = ControlSchema::parse(&schema).ok_or(MapError::UpnpSchema)?;
        // `igd-next` takes a path of the router's own address, else a whole URL.
        let reference = |url: &RouterUrl| {
            if url.addr() == location.addr() {
                url.path().to_owned()
            } else {
                url.whole()
            }
        };
        Self::of(igd_next::Gateway {
            addr: SocketAddr::V4(location.addr()),
            root_url: location.path().to_owned(),
            control_url: reference(description.control()),
            control_schema_url: reference(description.schema()),
            control_schema: schema.into_actions().collect(),
            service_type: description.service_type().to_owned(),
        })
    }

    /// `router`, with this host's address on its side.
    fn of(router: igd_next::Gateway) -> Result<Self, MapError> {
        let SocketAddr::V4(address) = router.addr else {
            unreachable!("a search of IPv4 and an announcement both name a router on IPv4");
        };
        let local = FacingSocket::connect(address).map_err(MapError::Io)?.host;
        Ok(Self { router, local })
    }

    /// The document at `url`, within `timeout` and `MAX_DOCUMENT` bytes, as `igd-next` reads one:
    /// no redirect followed.
    fn fetch(url: &RouterUrl, timeout: Duration) -> Result<Vec<u8>, MapError> {
        let response = attohttpc::get(url.whole())
            .follow_redirects(false)
            .connect_timeout(timeout)
            .timeout(timeout)
            .send()
            .map_err(MapError::UpnpFetch)?;
        let (_, _, reader) = response.split();
        let mut body = Vec::new();
        let most = u64::try_from(MAX_DOCUMENT).expect("a document's bound fits u64");
        reader
            .take(most + 1)
            .read_to_end(&mut body)
            .map_err(MapError::UpnpRead)?;
        if body.len() > MAX_DOCUMENT {
            return Err(MapError::UpnpRead(io::Error::from(
                io::ErrorKind::FileTooLarge,
            )));
        }
        Ok(body)
    }

    /// Maps as `ask` says: anew at the same port outside, or at any other one when another host
    /// holds that; or the renewed mapping again at its port outside. A router that takes only a
    /// lease that never ends gets one, which the mapping still renews within the lifetime, as a
    /// router that restarted forgets it. The map's stop ends it between two requests, each of
    /// which `igd-next` bounds at 10 s. The grant names no address outside: `address` reads it.
    pub(crate) fn map(&self, ask: Ask<'_>, stop: &AtomicBool) -> Result<Mapping, MapError> {
        let Ask {
            internal_port,
            renewed,
            lifetime,
        } = ask;
        running(stop)?;
        let local = SocketAddr::from((self.local, internal_port));
        let mut lease = u32::try_from(lifetime.as_secs())
            .expect("a mapping's lifetime fits UPnP's 32 bits of seconds");
        let external_port = renewed.map_or(internal_port, |mapping| mapping.external_port);
        let added = match self.add(external_port, local, lease) {
            Err(AddPortError::OnlyPermanentLeasesSupported) => {
                running(stop)?;
                lease = 0;
                self.add(external_port, local, lease)
            }
            added => added,
        };
        let external_port = match added {
            Ok(()) => external_port,
            Err(AddPortError::PortInUse) if renewed.is_none() => {
                running(stop)?;
                match self.add_any(local, lease) {
                    Err(AddAnyPortError::OnlyPermanentLeasesSupported) => {
                        running(stop)?;
                        self.add_any(local, 0)
                    }
                    added => added,
                }
                .map_err(MapError::UpnpAddAny)?
            }
            Err(error) => return Err(MapError::UpnpAdd(error)),
        };
        Ok(Mapping {
            external_port,
            address: None,
            lifetime,
            by: MappedBy::Upnp,
        })
    }

    /// The router's address outside.
    pub(crate) fn address(&self) -> Result<Ipv4Addr, MapError> {
        match self
            .router
            .get_external_ip()
            .map_err(MapError::UpnpExternal)?
        {
            IpAddr::V4(address) => Ok(address),
            address @ IpAddr::V6(_) => Err(MapError::ExternalNotIpv4(address)),
        }
    }

    /// Deletes `mapping`, which this router granted. `igd-next` bounds the request at 10 s.
    pub(crate) fn delete(&self, mapping: &Mapping) -> Result<(), MapError> {
        debug_assert_eq!(mapping.by, MappedBy::Upnp, "a mapping of this router");
        self.router
            .remove_port(PortMappingProtocol::UDP, mapping.external_port)
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

/// An error once the map stopped, which ends a turn between two requests.
fn running(stop: &AtomicBool) -> Result<(), MapError> {
    if stop.load(Ordering::SeqCst) {
        return Err(MapError::Stopped);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fake_router;

#[cfg(test)]
mod tests;
