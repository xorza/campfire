use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use campfire_common::ErrorReport;
use campfire_store::SharedPort;
use tracing::{debug, info, warn};

use crate::port_map::error::MapError;
use crate::port_map::gateway::{Ask, Gateway, Mapping, Tries, Wait};
use crate::port_map::ssdp_announcement::SsdpAnnouncement;
use crate::port_map::upnp_gateway::{SSDP, UpnpGateway};

/// The port PCP and NAT-PMP gateways answer on (RFC 6887 §19.1, RFC 6886 §3).
const PCP_PORT: u16 = 5351;
/// The least wait before a renewal, as RFC 6887 §11.2.1 has renewals never less than four seconds
/// apart; it holds a gateway that grants a very short lifetime off a loop of requests.
const RENEWAL_MIN: Duration = Duration::from_secs(4);

/// How long each part of the port map waits (11-quic-transport.md, A server maps its own port).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapTimes {
    /// The lifetime each mapping asks for.
    pub(crate) lifetime: Duration,
    /// How long the map waits for its next turn when no router mapped the port.
    pub(crate) retry: Duration,
    /// How PCP and NAT-PMP send a request again.
    pub(crate) tries: Tries,
    /// How long a UPnP search waits for a router.
    pub(crate) search: Duration,
    /// How long a PCP or NAT-PMP deletion waits for its answer.
    pub(crate) delete: Duration,
}

/// Where the map finds its gateway and its router, and hears routers' announcements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Routes {
    /// The default gateway the OS names at each turn, and SSDP's group, on port 1900, which the
    /// map shares with the host's other listeners.
    System,
    /// A gateway, a router and a listener of announcements at fixed addresses, a test's fakes.
    #[cfg(test)]
    At {
        pcp: SocketAddrV4,
        ssdp: SocketAddr,
        announce: SocketAddrV4,
    },
}

/// The port map's logic, which its thread runs a turn at a time: a mapping of `internal_port`
/// that it renews, or maps anew when it holds none or its renewal failed, by PCP or NAT-PMP at the
/// default gateway and else by UPnP.
#[derive(Debug)]
pub(crate) struct PortMapper {
    internal_port: u16,
    routes: Routes,
    times: MapTimes,
    held: Option<Held>,
    /// The last router that announced itself, which a turn maps through when no search finds a
    /// router.
    heard: Option<SsdpAnnouncement>,
    /// Whether the host was told that no router maps the port, since the last mapping.
    told: bool,
}

/// A mapping and the gateway or router that granted it.
#[derive(Debug)]
struct Held {
    by: Mapper,
    mapping: Mapping,
}

/// A gateway or router that maps a port.
#[derive(Debug)]
enum Mapper {
    Pmp(Gateway),
    Upnp(UpnpGateway),
}

/// Why a turn mapped no port: PCP's and NAT-PMP's error, and UPnP's.
#[derive(Debug)]
struct Unmapped {
    pmp: MapError,
    upnp: MapError,
}

impl MapTimes {
    /// A mapping of two hours, RFC 6886's recommended lifetime; a turn every 30 minutes while no
    /// router maps the port, as RFC 6887 §9 waits after a failure; RFC 6886's first wait of
    /// 250 ms, four tries, at most 3.75 s; a UPnP search of 3 s; and a deletion of at most a
    /// second, which a server's exit waits for.
    pub(crate) const CAMPFIRE: Self = Self {
        lifetime: Duration::from_secs(7200),
        retry: Duration::from_mins(30),
        tries: Tries {
            first: Duration::from_millis(250),
            count: 4,
        },
        search: Duration::from_secs(3),
        delete: Duration::from_secs(1),
    };
}

impl Routes {
    fn pcp(self) -> Result<SocketAddrV4, MapError> {
        match self {
            Self::System => {
                let gateway = netdev::get_default_gateway().map_err(MapError::NoGateway)?;
                let address = gateway.ipv4.first().ok_or(MapError::NoIpv4Gateway)?;
                Ok(SocketAddrV4::new(*address, PCP_PORT))
            }
            #[cfg(test)]
            Self::At { pcp, .. } => Ok(pcp),
        }
    }

    const fn ssdp(self) -> SocketAddr {
        match self {
            Self::System => SSDP,
            #[cfg(test)]
            Self::At { ssdp, .. } => ssdp,
        }
    }

    /// The socket that hears routers' announcements: on SSDP's port, joined to its group, beside
    /// the host's other listeners.
    pub(crate) fn announcements(self) -> io::Result<SharedPort> {
        match self {
            Self::System => {
                let IpAddr::V4(group) = SSDP.ip() else {
                    unreachable!("SSDP's group is IPv4");
                };
                let port = SharedPort::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SSDP.port()))?;
                port.socket()
                    .join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
                Ok(port)
            }
            #[cfg(test)]
            Self::At { announce, .. } => SharedPort::bind(announce),
        }
    }
}

impl PortMapper {
    pub(crate) const fn new(internal_port: u16, routes: Routes, times: MapTimes) -> Self {
        Self {
            internal_port,
            routes,
            times,
            held: None,
            heard: None,
            told: false,
        }
    }

    pub(crate) const fn routes(&self) -> Routes {
        self.routes
    }

    /// One turn: the mapping it holds renewed, or the port mapped anew when it holds none or the
    /// renewal failed, each grant held as the router makes it, so the map's end deletes it
    /// whether its address outside read or not. How long until the next turn: a renewal's time;
    /// when no router mapped the port, the lifetime of a PCP refusal, for which RFC 6887 §7.2 has
    /// a client expect the same answer, or else `retry`. A server started again meets such a
    /// refusal for the mapping its last run left, under that run's nonce, which forwards to the
    /// same port until it ends: so a refusal's wait is at most the lifetime the map asks for, as a
    /// router's refusal may last for years. `stop` ends the turn between two requests.
    pub(crate) fn turn(&mut self, stop: &AtomicBool) -> Duration {
        if let Some(held) = &mut self.held {
            let known = held.mapping.external();
            match held.renew(self.internal_port, self.times, stop) {
                Ok(()) => {
                    held.read_address(self.times, stop);
                    if held.mapping.external() != known {
                        Self::announce(&held.mapping);
                    }
                    return Self::renewal(held.mapping.lifetime, Self::draw());
                }
                Err(MapError::Stopped) => return Duration::ZERO,
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the router did not renew the server's port mapping; the port is mapped anew");
                    self.held = None;
                }
            }
        }
        match self.map_anew(stop) {
            Ok(held) => {
                let held = self.held.insert(held);
                held.read_address(self.times, stop);
                Self::announce(&held.mapping);
                self.told = false;
                Self::renewal(held.mapping.lifetime, Self::draw())
            }
            Err(_) if stop.load(Ordering::SeqCst) => Duration::ZERO,
            Err(unmapped) => {
                let pmp = ErrorReport::of(&unmapped.pmp);
                let upnp = ErrorReport::of(&unmapped.upnp);
                if self.told {
                    debug!(%pmp, %upnp, "no router mapped the server's port");
                } else {
                    warn!(port = self.internal_port, %pmp, %upnp, "no router mapped the server's port; a player outside reaches it only through a port its host forwards");
                    self.told = true;
                }
                match unmapped.pmp {
                    MapError::PcpRefused { lasts, .. } => {
                        lasts.clamp(RENEWAL_MIN, self.times.lifetime.max(RENEWAL_MIN))
                    }
                    _ => self.times.retry,
                }
            }
        }
    }

    /// Takes `datagram`, which came from `from` to the announcements' socket: whether it starts a
    /// turn at once, an announcement of a router at an address not heard before while no router
    /// maps the port, so a host that varies its location starts one turn at most. Each
    /// announcement replaces the one before, for the next turn.
    pub(crate) fn hear(&mut self, datagram: &[u8], from: SocketAddr) -> bool {
        let Some(announcement) = SsdpAnnouncement::parse(datagram, from) else {
            return false;
        };
        let ip = |heard: &SsdpAnnouncement| *heard.location().addr().ip();
        let new = self
            .heard
            .as_ref()
            .is_none_or(|heard| ip(heard) != ip(&announcement));
        self.heard = Some(announcement);
        new && self.held.is_none()
    }

    /// Deletes the mapping it holds, if any, at the server's end.
    pub(crate) fn end(&mut self) {
        let Some(held) = self.held.take() else {
            return;
        };
        let deleted = match held.by {
            Mapper::Pmp(gateway) => {
                gateway.delete(self.internal_port, &held.mapping, self.times.delete)
            }
            Mapper::Upnp(router) => router.delete(&held.mapping),
        };
        if let Err(error) = deleted {
            warn!(error = %ErrorReport::of(&error), external_port = held.mapping.external_port, "the router did not delete the server's port mapping; it ends with its lifetime");
        }
    }

    fn map_anew(&self, stop: &AtomicBool) -> Result<Held, Unmapped> {
        let pmp = match self.map_by_pmp(stop) {
            Ok(held) => return Ok(held),
            Err(error) => error,
        };
        match self.map_by_upnp(stop) {
            Ok(held) => Ok(held),
            Err(upnp) => Err(Unmapped { pmp, upnp }),
        }
    }

    fn map_by_pmp(&self, stop: &AtomicBool) -> Result<Held, MapError> {
        let mut gateway = Gateway::connect(self.routes.pcp()?).map_err(MapError::Io)?;
        let ask = Ask {
            internal_port: self.internal_port,
            renewed: None,
            lifetime: self.times.lifetime,
        };
        let wait = Wait {
            tries: self.times.tries,
            stop,
        };
        let mapping = gateway.map(ask, wait)?;
        Ok(Held {
            by: Mapper::Pmp(gateway),
            mapping,
        })
    }

    /// Maps the port through the router a search finds, else through the last that announced
    /// itself, whose answer a host firewall may have dropped (11-quic-transport.md, A server maps
    /// its own port).
    fn map_by_upnp(&self, stop: &AtomicBool) -> Result<Held, MapError> {
        if stop.load(Ordering::SeqCst) {
            return Err(MapError::Stopped);
        }
        let router = match UpnpGateway::search(self.routes.ssdp(), self.times.search) {
            Ok(router) => router,
            Err(searched) => match &self.heard {
                Some(heard) => UpnpGateway::announced(heard, self.times.search, stop)?,
                None => return Err(searched),
            },
        };
        let ask = Ask {
            internal_port: self.internal_port,
            renewed: None,
            lifetime: self.times.lifetime,
        };
        let mapping = router.map(ask, stop)?;
        Ok(Held {
            by: Mapper::Upnp(router),
            mapping,
        })
    }

    /// Logs where a player outside joins through `mapping`, or that another NAT stands before the
    /// router, whose own address is then private.
    fn announce(mapping: &Mapping) {
        let Some(external) = mapping.external() else {
            return;
        };
        if Self::is_public(*external.ip()) {
            info!(%external, by = ?mapping.by, "the router forwards the server's port; a player outside joins at this address");
        } else {
            warn!(%external, by = ?mapping.by, "the router forwards the server's port, but its own address is private: another NAT stands before it, and a player outside joins only if that NAT forwards the port to the router");
        }
    }

    /// Whether a router's external address is one the internet routes to: none of the private
    /// ranges of RFC 1918, carrier-grade NAT's 100.64.0.0/10 of RFC 6598, link-local, loopback or
    /// unspecified.
    const fn is_public(address: Ipv4Addr) -> bool {
        let [first, second, ..] = address.octets();
        let carrier_grade = first == 100 && second & 0xc0 == 64;
        !(address.is_private()
            || carrier_grade
            || address.is_link_local()
            || address.is_loopback()
            || address.is_unspecified())
    }

    /// When a mapping of `lifetime` is renewed: at a point between 1/2 and 5/8 of it that `draw`
    /// picks, 0 the earliest and `u16::MAX` the latest, as RFC 6887 §11.2.1 has it; never sooner
    /// than `RENEWAL_MIN`.
    fn renewal(lifetime: Duration, draw: u16) -> Duration {
        let span = u128::from(u16::MAX);
        let nanos = lifetime.as_nanos() * (4 * span + u128::from(draw)) / (8 * span);
        Duration::from_nanos(u64::try_from(nanos).expect("a renewal's time fits u64 nanoseconds"))
            .max(RENEWAL_MIN)
    }

    fn draw() -> u16 {
        let mut bytes = [0; 2];
        getrandom::fill(&mut bytes).expect("the OS gives random bytes");
        u16::from_le_bytes(bytes)
    }
}

impl Held {
    /// Renews the mapping, as its gateway or router grants it again, its address outside to read
    /// again.
    fn renew(
        &mut self,
        internal_port: u16,
        times: MapTimes,
        stop: &AtomicBool,
    ) -> Result<(), MapError> {
        let ask = Ask {
            internal_port,
            renewed: Some(&self.mapping),
            lifetime: times.lifetime,
        };
        let mapping = match &mut self.by {
            Mapper::Pmp(gateway) => gateway.map(
                ask,
                Wait {
                    tries: times.tries,
                    stop,
                },
            ),
            Mapper::Upnp(router) => router.map(ask, stop),
        }?;
        self.mapping = mapping;
        Ok(())
    }

    /// Reads the mapping's address outside, which a PCP grant names already; one that does not
    /// read is logged, and the mapping is held still, for its renewal and its deletion.
    fn read_address(&mut self, times: MapTimes, stop: &AtomicBool) {
        if self.mapping.address.is_some() {
            return;
        }
        let address = match &self.by {
            Mapper::Pmp(gateway) => gateway.address(Wait {
                tries: times.tries,
                stop,
            }),
            Mapper::Upnp(router) => router.address(),
        };
        match address {
            Ok(address) => self.mapping.address = Some(address),
            Err(MapError::Stopped) => {}
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), external_port = self.mapping.external_port, "the router forwards the server's port, but its address outside did not read; the map renews the mapping");
            }
        }
    }
}

#[cfg(test)]
mod tests;
