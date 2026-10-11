use std::net::{IpAddr, SocketAddr};
use std::sync::mpsc::{self, RecvTimeoutError};

use campfire_store::Worker;
use tracing::info;

use crate::port_mapper::{MapTimes, PortMapper, Routes};

/// A server's port, which its router forwards from outside, on a thread of its own (design 15,
/// D12): it maps the port by PCP or NAT-PMP at the default gateway, else by UPnP, renews the
/// mapping, maps it anew when a renewal fails, and tries again while no router maps it, in 30
/// minutes or when a PCP refusal's lifetime ends, logging each change. Dropped, it ends the thread after its turn, which deletes the
/// mapping, and joins it.
#[derive(Debug)]
pub struct PortMap {
    /// Dropped first, which wakes the thread to its end.
    _stop: mpsc::Sender<()>,
    _worker: Worker,
}

impl PortMap {
    /// The map of the server socket at `local`, at this host's gateway or router; none, logged,
    /// for a socket no router forwards to.
    pub fn start(local: SocketAddr) -> Option<PortMap> {
        if !PortMap::forwardable(local) {
            info!(%local, "the server listens where no router forwards; its port is not mapped");
            return None;
        }
        Some(PortMap::of(PortMapper::new(
            local.port(),
            Routes::System,
            MapTimes::CAMPFIRE,
        )))
    }

    /// Whether a router forwards to a socket at `local`: one bound to IPv4, all of the host's
    /// addresses or one not loopback. A socket bound to IPv6 takes IPv4 only where its OS makes
    /// it dual-stack by default, as Linux does and Windows does not.
    fn forwardable(local: SocketAddr) -> bool {
        matches!(local.ip(), IpAddr::V4(ip) if !ip.is_loopback())
    }

    fn of(mut mapper: PortMapper) -> PortMap {
        let (stop, stopped) = mpsc::channel::<()>();
        let worker = Worker::start("campfire-port-map", move || {
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(mapper.turn()) {}
            mapper.end();
        });
        PortMap {
            _stop: stop,
            _worker: worker,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, UdpSocket};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::gateway::Tries;
    use crate::gateway::fake_gateway::FakeGateway;

    #[test]
    fn a_dropped_map_wakes_its_thread_which_deletes_the_mapping() {
        let external = SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50_000);
        let gateway =
            FakeGateway::start(move |request| vec![FakeGateway::pcp_reply(request, 0, external)]);
        let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let routes = Routes::At {
            pcp: gateway.address,
            ssdp: silent.local_addr().unwrap(),
        };
        let times = MapTimes {
            tries: Tries {
                first: Duration::from_millis(20),
                count: 2,
            },
            delete: Duration::from_millis(100),
            ..MapTimes::CAMPFIRE
        };
        let map = PortMap::of(PortMapper::new(4433, routes, times));
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
}
