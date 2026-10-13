use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use campfire_log::ErrorReport;
use ring::rand::{SecureRandom, SystemRandom};
use tracing::{debug, info, warn};

use crate::error::MapError;
use crate::gateway::{Gateway, Mapping, Tries};
use crate::upnp_gateway::{SSDP, UpnpGateway};

/// The port PCP and NAT-PMP gateways answer on (RFC 6887 §19.1, RFC 6886 §3).
const PCP_PORT: u16 = 5351;
/// The least wait before a renewal, as RFC 6887 §11.2.1 has renewals never less than four seconds
/// apart; it holds a gateway that grants a very short lifetime off a loop of requests.
const RENEWAL_MIN: Duration = Duration::from_secs(4);

/// How long each part of the port map waits (design 15, D12).
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

/// Where the map finds its gateway and its router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Routes {
    /// The default gateway the OS names at each turn, and SSDP's group.
    System,
    /// A gateway and a router at fixed addresses, a test's fakes.
    #[cfg(test)]
    At { pcp: SocketAddrV4, ssdp: SocketAddr },
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
    pub(crate) const CAMPFIRE: MapTimes = MapTimes {
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
            Routes::System => {
                let gateway = netdev::get_default_gateway().map_err(MapError::NoGateway)?;
                let address = gateway.ipv4.first().ok_or_else(|| {
                    MapError::NoGateway(String::from("the default gateway has no IPv4 address"))
                })?;
                Ok(SocketAddrV4::new(*address, PCP_PORT))
            }
            #[cfg(test)]
            Routes::At { pcp, .. } => Ok(pcp),
        }
    }

    const fn ssdp(self) -> SocketAddr {
        match self {
            Routes::System => SSDP,
            #[cfg(test)]
            Routes::At { ssdp, .. } => ssdp,
        }
    }
}

impl PortMapper {
    pub(crate) const fn new(internal_port: u16, routes: Routes, times: MapTimes) -> PortMapper {
        PortMapper {
            internal_port,
            routes,
            times,
            held: None,
            told: false,
        }
    }

    /// One turn: the mapping it holds renewed, or the port mapped anew when it holds none or the
    /// renewal failed. How long until the next turn: a renewal's time; when no router mapped the
    /// port, the lifetime of a PCP refusal, for which RFC 6887 §7.2 has a client expect the same
    /// answer, or else `retry`. A server started again meets such a refusal for the mapping its
    /// last run left, under that run's nonce, which forwards to the same port until it ends.
    pub(crate) fn turn(&mut self) -> Duration {
        if let Some(held) = &mut self.held {
            match held.renew(self.internal_port, self.times) {
                Ok(mapping) => {
                    if mapping.external != held.mapping.external {
                        PortMapper::announce(&mapping);
                    }
                    held.mapping = mapping;
                    return PortMapper::renewal(mapping.lifetime, PortMapper::draw());
                }
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the router did not renew the server's port mapping; the port is mapped anew");
                    self.held = None;
                }
            }
        }
        match self.map_anew() {
            Ok(held) => {
                PortMapper::announce(&held.mapping);
                self.told = false;
                let wait = PortMapper::renewal(held.mapping.lifetime, PortMapper::draw());
                self.held = Some(held);
                wait
            }
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
                    MapError::PcpRefused { lasts, .. } => lasts.max(RENEWAL_MIN),
                    _ => self.times.retry,
                }
            }
        }
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
            warn!(error = %ErrorReport::of(&error), external = %held.mapping.external, "the router did not delete the server's port mapping; it ends with its lifetime");
        }
    }

    fn map_anew(&self) -> Result<Held, Unmapped> {
        let pmp = match self.map_by_pmp() {
            Ok(held) => return Ok(held),
            Err(error) => error,
        };
        match self.map_by_upnp() {
            Ok(held) => Ok(held),
            Err(upnp) => Err(Unmapped { pmp, upnp }),
        }
    }

    fn map_by_pmp(&self) -> Result<Held, MapError> {
        let mut gateway = Gateway::connect(self.routes.pcp()?).map_err(MapError::Io)?;
        let mapping = gateway.map(
            self.internal_port,
            None,
            self.times.lifetime,
            self.times.tries,
        )?;
        Ok(Held {
            by: Mapper::Pmp(gateway),
            mapping,
        })
    }

    fn map_by_upnp(&self) -> Result<Held, MapError> {
        let router = UpnpGateway::search(self.routes.ssdp(), self.times.search)?;
        let mapping = router.map(self.internal_port, None, self.times.lifetime)?;
        Ok(Held {
            by: Mapper::Upnp(router),
            mapping,
        })
    }

    /// Logs where a player outside joins through `mapping`, or that another NAT stands before the
    /// router, whose own address is then private.
    fn announce(mapping: &Mapping) {
        let external = mapping.external;
        if PortMapper::public(*external.ip()) {
            info!(%external, by = ?mapping.by, "the router forwards the server's port; a player outside joins at this address");
        } else {
            warn!(%external, by = ?mapping.by, "the router forwards the server's port, but its own address is private: another NAT stands before it, and a player outside joins only if that NAT forwards the port to the router");
        }
    }

    /// Whether a router's external address is one the internet routes to: none of the private
    /// ranges of RFC 1918, carrier-grade NAT's 100.64.0.0/10 of RFC 6598, link-local, loopback or
    /// unspecified.
    const fn public(address: Ipv4Addr) -> bool {
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
        SystemRandom::new()
            .fill(&mut bytes)
            .expect("the OS gives random bytes");
        u16::from_le_bytes(bytes)
    }
}

impl Held {
    fn renew(&mut self, internal_port: u16, times: MapTimes) -> Result<Mapping, MapError> {
        match &mut self.by {
            Mapper::Pmp(gateway) => gateway.map(
                internal_port,
                Some(&self.mapping),
                times.lifetime,
                times.tries,
            ),
            Mapper::Upnp(router) => router.map(internal_port, Some(&self.mapping), times.lifetime),
        }
    }
}

#[cfg(test)]
mod tests;
