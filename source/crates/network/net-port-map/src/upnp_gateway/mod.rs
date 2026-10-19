use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use campfire_common::ErrorReport;

use campfire_net_core::Random;
use tracing::debug;
use ureq::Agent;
use xmltree::Element;

use crate::control_schema::ControlSchema;
use crate::error::{MapError, SoapError};
use crate::facing_socket::FacingSocket;
use crate::gateway::{Ask, MappedBy, Mapping};
use crate::router_description::RouterDescription;
use crate::router_url::RouterUrl;
use crate::soap_call::SoapCall;
use crate::ssdp_announcement::SsdpAnnouncement;
use crate::ssdp_search::SsdpSearch;

/// The SSDP address a UPnP router answers a search on.
pub(crate) const SSDP: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 1900);
/// The most bytes of a router's document or reply the map reads, as `igd-next` bounded them.
const MAX_DOCUMENT: u64 = 1 << 20;
/// How long one action waits for the router's reply, as `igd-next` waited.
const ACTION_TIMEOUT: Duration = Duration::from_secs(10);
/// How many ports the map draws for a router that lacks `AddAnyPortMapping`, as `igd-next` drew.
const DRAWS: usize = 20;
/// The UPnP error codes the map tells apart (UPnP-IGD's `WANIPConnection:1` §2.4.16).
const CONFLICT_IN_MAPPING_ENTRY: u16 = 718;
const SAME_PORT_VALUES_REQUIRED: u16 = 724;
const ONLY_PERMANENT_LEASES_SUPPORTED: u16 = 725;

/// A UPnP-IGD router: its WAN connection service, read from its description and its schema, and
/// the address of this host on its side (13-net-stack/port-map.md, A server maps its own port).
#[derive(Debug)]
pub(crate) struct UpnpGateway {
    http: Agent,
    service_type: String,
    control: RouterUrl,
    schema: ControlSchema,
    local: Ipv4Addr,
}

/// What a mapping asks a router for: the port inside, a lease of `seconds`, 0 for one that never
/// ends, and the name the router lists it by.
#[derive(Debug, Clone, Copy)]
struct Lease<'a> {
    internal_port: u16,
    seconds: u32,
    name: &'a str,
}

impl UpnpGateway {
    /// The first router on IPv4 that answers a search sent to `ssdp` and whose documents read,
    /// all within `timeout`, as `igd-next` bounded its search. A router whose documents do not
    /// read is passed over for the next that answers. The map's stop ends it between two reads.
    pub(crate) fn search(
        ssdp: SocketAddr,
        timeout: Duration,
        stop: &AtomicBool,
    ) -> Result<Self, MapError> {
        let search = SsdpSearch::send(ssdp, timeout).map_err(MapError::Io)?;
        while let Some(location) = search.next_location() {
            match Self::at(&location, || search.remaining(), stop) {
                Ok(router) => return Ok(router),
                Err(MapError::Stopped) => return Err(MapError::Stopped),
                Err(error) => {
                    debug!(error = %ErrorReport::of(&error), location = %location.whole(), "a UPnP router answered, but its documents did not read");
                }
            }
        }
        Err(MapError::NoUpnpRouter)
    }

    /// The router of `announcement`, each of its documents read within `timeout`. The map's stop
    /// ends it between the two reads.
    pub(crate) fn announced(
        announcement: &SsdpAnnouncement,
        timeout: Duration,
        stop: &AtomicBool,
    ) -> Result<Self, MapError> {
        Self::at(announcement.location(), || timeout, stop)
    }

    /// The router whose description is at `location`: its description and its service schema,
    /// each read within the time `within` gives as the read starts, every URL at its address.
    fn at(
        location: &RouterUrl,
        within: impl Fn() -> Duration,
        stop: &AtomicBool,
    ) -> Result<Self, MapError> {
        // As `igd-next`'s client did, it follows no redirect, so each URL is at the address the
        // router was found at, and it keeps no connection, which a router may close between two
        // requests, and a SOAP action is not sent again.
        let http = Agent::config_builder()
            .max_redirects(0)
            .max_idle_connections(0)
            .build()
            .new_agent();
        let description = Self::fetch(&http, location, within())?;
        let description =
            RouterDescription::parse(&description, location).ok_or(MapError::UpnpDescription)?;
        running(stop)?;
        let schema = Self::fetch(&http, description.schema(), within())?;
        let schema = ControlSchema::parse(&schema).ok_or(MapError::UpnpSchema)?;
        let local = FacingSocket::connect(location.addr())
            .map_err(MapError::Io)?
            .host;
        Ok(Self {
            http,
            service_type: description.service_type().to_owned(),
            control: description.control().clone(),
            schema,
            local,
        })
    }

    /// The document at `url`, within `timeout` and `MAX_DOCUMENT` bytes.
    fn fetch(http: &Agent, url: &RouterUrl, timeout: Duration) -> Result<Vec<u8>, ureq::Error> {
        let mut response = http
            .get(url.whole())
            .config()
            .timeout_global(Some(timeout))
            .build()
            .call()?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_DOCUMENT)
            .read_to_vec()
    }

    /// Maps as `ask` says: anew at the same port outside, or at any other one when another host
    /// holds that; or the renewed mapping again at its port outside. A router that takes only a
    /// lease that never ends gets one, which the mapping still renews within the lifetime, as a
    /// router that restarted forgets it. A port drawn comes from `random`. The map's stop ends it
    /// between two requests, each of which waits at most `ACTION_TIMEOUT`. The grant names no
    /// address outside: `address` reads it.
    pub(crate) fn map(
        &self,
        ask: Ask<'_>,
        stop: &AtomicBool,
        random: &mut Random,
    ) -> Result<Mapping, MapError> {
        let Ask {
            internal_port,
            name,
            renewed,
            lifetime,
        } = ask;
        running(stop)?;
        let mut lease = Lease {
            internal_port,
            seconds: u32::try_from(lifetime.as_secs())
                .expect("a mapping's lifetime fits UPnP's 32 bits of seconds"),
            name,
        };
        let external_port = renewed.map_or(internal_port, |mapping| mapping.external_port);
        let added = match self.add(external_port, lease) {
            Err(SoapError::Refused(ONLY_PERMANENT_LEASES_SUPPORTED)) => {
                running(stop)?;
                lease.seconds = 0;
                self.add(external_port, lease)
            }
            added => added,
        };
        let external_port = match added {
            Ok(()) => external_port,
            Err(SoapError::Refused(CONFLICT_IN_MAPPING_ENTRY)) if renewed.is_none() => {
                running(stop)?;
                match self.add_any(lease, stop, random) {
                    Err(MapError::UpnpAddAny(SoapError::Refused(
                        ONLY_PERMANENT_LEASES_SUPPORTED,
                    ))) => {
                        running(stop)?;
                        lease.seconds = 0;
                        self.add_any(lease, stop, random)
                    }
                    added => added,
                }?
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
        let reply = self
            .call("GetExternalIPAddress", &[], |_| None)
            .map_err(MapError::UpnpExternal)?;
        let address = reply
            .get_child("NewExternalIPAddress")
            .and_then(Element::get_text)
            .and_then(|address| address.trim().parse().ok())
            .ok_or(MapError::UpnpExternal(SoapError::Reply))?;
        match address {
            IpAddr::V4(address) => Ok(address),
            address @ IpAddr::V6(_) => Err(MapError::ExternalNotIpv4(address)),
        }
    }

    /// Deletes `mapping`, which this router granted, waiting at most `ACTION_TIMEOUT`.
    pub(crate) fn delete(&self, mapping: &Mapping) -> Result<(), MapError> {
        debug_assert_eq!(mapping.by, MappedBy::Upnp, "a mapping of this router");
        let arguments = self
            .schema
            .arguments("DeletePortMapping")
            .ok_or(MapError::UpnpRemove(SoapError::Unsupported))?;
        let external_port = mapping.external_port.to_string();
        self.call("DeletePortMapping", arguments, |name| match name {
            "NewRemoteHost" => Some(String::new()),
            "NewExternalPort" => Some(external_port.clone()),
            "NewProtocol" => Some(String::from("UDP")),
            _ => None,
        })
        .map_err(MapError::UpnpRemove)?;
        Ok(())
    }

    /// Maps `lease` at a port outside the router picks, by `AddAnyPortMapping`, or else, as
    /// `igd-next` did, at a port drawn: past up to `DRAWS` ports another host holds, and at the
    /// port inside for a router that maps only a port to the same one.
    fn add_any(
        &self,
        lease: Lease<'_>,
        stop: &AtomicBool,
        random: &mut Random,
    ) -> Result<u16, MapError> {
        if let Some(arguments) = self.schema.arguments("AddAnyPortMapping") {
            let wished = Self::draw(random);
            let reply = self
                .call("AddAnyPortMapping", arguments, |name| {
                    self.value(name, wished, lease)
                })
                .map_err(MapError::UpnpAddAny)?;
            return reply
                .get_child("NewReservedPort")
                .and_then(Element::get_text)
                .and_then(|port| port.trim().parse().ok())
                .filter(|&port| port != 0)
                .ok_or(MapError::UpnpAddAny(SoapError::Reply));
        }
        for _ in 0..DRAWS {
            running(stop)?;
            let external_port = Self::draw(random);
            match self.add(external_port, lease) {
                Ok(()) => return Ok(external_port),
                Err(SoapError::Refused(CONFLICT_IN_MAPPING_ENTRY)) => {}
                Err(SoapError::Refused(SAME_PORT_VALUES_REQUIRED)) => {
                    running(stop)?;
                    return self
                        .add(lease.internal_port, lease)
                        .map(|()| lease.internal_port)
                        .map_err(MapError::UpnpAddAny);
                }
                Err(error) => return Err(MapError::UpnpAddAny(error)),
            }
        }
        Err(MapError::UpnpAddAny(SoapError::Refused(
            CONFLICT_IN_MAPPING_ENTRY,
        )))
    }

    /// Maps `lease` at `external_port`.
    fn add(&self, external_port: u16, lease: Lease<'_>) -> Result<(), SoapError> {
        let arguments = self
            .schema
            .arguments("AddPortMapping")
            .ok_or(SoapError::Unsupported)?;
        self.call("AddPortMapping", arguments, |name| {
            self.value(name, external_port, lease)
        })?;
        Ok(())
    }

    /// The value of a mapping's argument `name`, for `lease` at `external_port`; none for one the
    /// map does not know.
    fn value(&self, name: &str, external_port: u16, lease: Lease<'_>) -> Option<String> {
        Some(match name {
            "NewRemoteHost" => String::new(),
            "NewExternalPort" => external_port.to_string(),
            "NewProtocol" => String::from("UDP"),
            "NewInternalPort" => lease.internal_port.to_string(),
            "NewInternalClient" => self.local.to_string(),
            "NewEnabled" => String::from("1"),
            "NewPortMappingDescription" => lease.name.to_owned(),
            "NewLeaseDuration" => lease.seconds.to_string(),
            _ => return None,
        })
    }

    /// Takes `action` at the router, its `arguments` with the values `value` gives them, and waits
    /// at most `ACTION_TIMEOUT` for the reply: the action's response, or why the router did not
    /// take it. A reply of an HTTP error is read too, as a SOAP fault comes with status 500.
    fn call(
        &self,
        action: &str,
        arguments: &[String],
        value: impl Fn(&str) -> Option<String>,
    ) -> Result<Element, SoapError> {
        let call = SoapCall {
            service_type: &self.service_type,
            action,
        };
        let mut reply = self
            .http
            .post(self.control.whole())
            .header("SOAPAction", call.header())
            .header("Content-Type", "text/xml")
            .config()
            .timeout_global(Some(ACTION_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .send(call.envelope(arguments, value))?;
        let body = reply
            .body_mut()
            .with_config()
            .limit(MAX_DOCUMENT)
            .read_to_vec()?;
        call.reply(&body)
    }

    /// A port outside of the upper half, 32,768 to 65,535, drawn from `random`, as `igd-next` drew
    /// one there.
    fn draw(random: &mut Random) -> u16 {
        0x8000 | u16::from_le_bytes(random.array())
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
