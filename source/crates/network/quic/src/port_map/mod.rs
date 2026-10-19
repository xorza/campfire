mod control_schema;
mod error;
mod facing_socket;
mod gateway;
mod nat_pmp_map;
mod pcp_map;
mod port_mapper;
mod reply_error;
mod router_description;
mod router_url;
mod router_xml;
mod ssdp_announcement;
mod upnp_gateway;

use std::io::{self, ErrorKind};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use campfire_common::ErrorReport;
use campfire_store::{SharedPort, Worker};
use polling::{Event, Events, Poller};
use tracing::{error, info, warn};

use crate::port_map::port_mapper::{MapTimes, PortMapper, Routes};

/// The poller's key of the announcements' socket.
const ANNOUNCEMENTS: usize = 0;
/// The most bytes of an announcement the thread reads: a datagram of a network's MTU.
const DATAGRAM: usize = 1500;

/// A server's port, which its router forwards from outside, on a thread of its own
/// (15-quic-transport.md, A server maps its own port): it maps the port by PCP or NAT-PMP at the
/// default gateway, else by UPnP, renews the mapping, maps it anew when a renewal fails, and tries
/// again while no router maps it, in 30 minutes, when a PCP refusal's lifetime ends, or at once
/// when a router at an address not heard before announces itself, logging each change. Dropped, it
/// stops the thread's turn before its next request, has the thread delete the mapping, and joins
/// it.
#[derive(Debug)]
pub struct PortMap {
    stop: Arc<Stop>,
    _worker: Worker,
}

/// The thread's stop: a flag the drop raises, and the poller it waits on, which the drop wakes.
#[derive(Debug)]
struct Stop {
    stopped: AtomicBool,
    poller: Poller,
}

/// What the thread waits on between its turns: the stop, and routers' announcements on their
/// socket, when it opened.
#[derive(Debug)]
struct Listening {
    stop: Arc<Stop>,
    announcements: Option<SharedPort>,
    events: Events,
    datagram: Box<[u8; DATAGRAM]>,
}

impl PortMap {
    /// The map of the server socket at `local`, at this host's gateway or router; none, logged,
    /// for a socket no router forwards to, or when its thread cannot wait.
    pub fn start(local: SocketAddr) -> Option<Self> {
        if !Self::forwardable(local) {
            info!(%local, "the server listens where no router forwards; its port is not mapped");
            return None;
        }
        let mapper = PortMapper::new(local.port(), Routes::System, MapTimes::CAMPFIRE);
        match Self::of(mapper) {
            Ok(map) => Some(map),
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), "the port map's thread cannot wait; the server's port is not mapped");
                None
            }
        }
    }

    /// Whether a router forwards to a socket at `local`: one bound to IPv4, all of the host's
    /// addresses or one not loopback. A socket bound to IPv6 takes IPv4 only where its OS makes
    /// it dual-stack by default, as Linux does and Windows does not.
    const fn forwardable(local: SocketAddr) -> bool {
        matches!(local.ip(), IpAddr::V4(ip) if !ip.is_loopback())
    }

    fn of(mapper: PortMapper) -> io::Result<Self> {
        let stop = Arc::new(Stop {
            stopped: AtomicBool::new(false),
            poller: Poller::new()?,
        });
        let listening = Listening::open(Arc::clone(&stop), mapper.routes())?;
        let worker = Worker::start("campfire-port-map", move || listening.run(mapper));
        Ok(Self {
            stop,
            _worker: worker,
        })
    }
}

impl Drop for PortMap {
    fn drop(&mut self) {
        self.stop.stopped.store(true, Ordering::SeqCst);
        if let Err(error) = self.stop.poller.notify() {
            warn!(error = %ErrorReport::of(&error), "the port map's thread was not woken; it ends after its wait");
        }
    }
}

impl Listening {
    /// The wait of `stop`, and the announcements' socket of `routes`, non-blocking and watched;
    /// a socket that does not open is logged, and the map hears no announcement.
    fn open(stop: Arc<Stop>, routes: Routes) -> io::Result<Self> {
        let announcements = match routes.announcements() {
            Ok(port) => {
                port.socket().set_nonblocking(true)?;
                // SAFETY: `Listening::drop` deletes the socket from this poller before it drops.
                unsafe {
                    stop.poller
                        .add(port.socket(), Event::readable(ANNOUNCEMENTS))?;
                }
                Some(port)
            }
            Err(error) => {
                info!(error = %ErrorReport::of(&error), "the server hears no router's announcement; it finds a router by its searches alone");
                None
            }
        };
        Ok(Self {
            stop,
            announcements,
            events: Events::new(),
            datagram: Box::new([0; DATAGRAM]),
        })
    }

    /// The thread: a turn of `mapper`, then a wait until its next, which a stop ends, and an
    /// announcement that starts a turn cuts short; at the stop, the mapping deleted.
    fn run(mut self, mut mapper: PortMapper) {
        while self.turn(&mut mapper) {}
        mapper.end();
    }

    /// A turn of `mapper` and the wait after it: whether another turn comes, which none does once
    /// stopped, or once the wait fails.
    fn turn(&mut self, mapper: &mut PortMapper) -> bool {
        let next = Instant::now() + mapper.turn(&self.stop.stopped);
        loop {
            if self.stop.stopped.load(Ordering::SeqCst) {
                return false;
            }
            let timeout = next.saturating_duration_since(Instant::now());
            if timeout.is_zero() {
                return true;
            }
            if let Some(port) = &self.announcements
                && let Err(error) = self
                    .stop
                    .poller
                    .modify(port.socket(), Event::readable(ANNOUNCEMENTS))
            {
                warn!(error = %ErrorReport::of(&error), "the port map's thread cannot watch for announcements");
            }
            self.events.clear();
            if let Err(error) = self.stop.poller.wait(&mut self.events, Some(timeout)) {
                error!(error = %ErrorReport::of(&error), "the port map's thread cannot wait; it deletes its mapping and ends");
                return false;
            }
            if self.hear(mapper) {
                return true;
            }
        }
    }

    /// Reads each announcement that waits: whether one starts a turn at once.
    fn hear(&mut self, mapper: &mut PortMapper) -> bool {
        let Some(port) = &self.announcements else {
            return false;
        };
        let mut starts = false;
        loop {
            match port.socket().recv_from(self.datagram.as_mut_slice()) {
                Ok((read, from)) => starts |= mapper.hear(&self.datagram[..read], from),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return starts,
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the announcements' socket failed");
                    return starts;
                }
            }
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        if let Some(port) = &self.announcements {
            self.stop.poller.delete(port.socket()).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, UdpSocket};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::port_map::gateway::Tries;
    use crate::port_map::gateway::fake_gateway::FakeGateway;
    use crate::port_map::upnp_gateway::fake_router::{Answer, FakeRouter};

    #[test]
    fn a_dropped_map_wakes_its_thread_which_deletes_the_mapping() {
        let external = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000);
        let gateway =
            FakeGateway::start(move |request| vec![FakeGateway::pcp_reply(request, 0, external)]);
        let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let routes = Routes::At {
            pcp: gateway.address,
            ssdp: silent.local_addr().unwrap(),
            announce: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        };
        let times = MapTimes {
            tries: Tries {
                first: Duration::from_millis(20),
                count: 2,
            },
            delete: Duration::from_millis(100),
            ..MapTimes::CAMPFIRE
        };
        let map = PortMap::of(PortMapper::new(4433, routes, times)).unwrap();
        let start = Instant::now();
        while gateway.requests().is_empty() {
            assert!(
                start.elapsed() < Duration::from_millis(500),
                "the map did not ask"
            );
            thread::sleep(Duration::from_millis(1));
        }
        // The thread waits an hour to renew; the drop ends it at once, with the deletion.
        let start = Instant::now();
        drop(map);
        assert!(start.elapsed() < Duration::from_millis(200));
        let requests = gateway.requests();
        assert_eq!(requests.len(), 2);
        // The deletion: a lifetime of 0.
        assert_eq!(requests[1][4..8], [0; 4]);
    }

    #[test]
    fn a_router_forwards_to_a_socket_bound_to_ipv4_and_not_loopback() {
        for (ip, forwardable) in [
            (IpAddr::V4(Ipv4Addr::UNSPECIFIED), true),
            (IpAddr::V4(Ipv4Addr::new(192, 168, 0, 5)), true),
            (IpAddr::V4(Ipv4Addr::LOCALHOST), false),
            (IpAddr::V6(Ipv6Addr::UNSPECIFIED), false),
            (IpAddr::V6(Ipv6Addr::LOCALHOST), false),
            (
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
                false,
            ),
        ] {
            assert_eq!(
                PortMap::forwardable(SocketAddr::new(ip, 4433)),
                forwardable,
                "{ip}"
            );
        }
    }

    #[test]
    fn an_announcement_cuts_the_wait_after_a_turn_that_mapped_nothing() {
        // No PCP server, and a search no router answers: the turn maps nothing, and waits its
        // retry of 30 minutes, which a router's announcement cuts short.
        let router = FakeRouter::start(|_| Answer::Done);
        let closed = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let SocketAddr::V4(pcp) = closed.local_addr().unwrap() else {
            unreachable!("a socket bound to IPv4 has an IPv4 address");
        };
        drop(closed);
        let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let SocketAddr::V4(announce) = listener.local_addr().unwrap() else {
            unreachable!("a socket bound to IPv4 has an IPv4 address");
        };
        drop(listener);
        let routes = Routes::At {
            pcp,
            ssdp: silent.local_addr().unwrap(),
            announce,
        };
        let times = MapTimes {
            tries: Tries {
                first: Duration::from_millis(20),
                count: 2,
            },
            search: Duration::from_millis(50),
            delete: Duration::from_millis(100),
            ..MapTimes::CAMPFIRE
        };
        let map = PortMap::of(PortMapper::new(4433, routes, times)).unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        sender.send_to(&router.announcement(), announce).unwrap();
        let start = Instant::now();
        while !router
            .calls()
            .iter()
            .any(|call| call.action == "AddPortMapping")
        {
            assert!(
                start.elapsed() < Duration::from_millis(800),
                "the announcement started no turn"
            );
            thread::sleep(Duration::from_millis(1));
        }
        drop(map);
        assert_eq!(router.calls().last().unwrap().action, "DeletePortMapping");
    }
}
