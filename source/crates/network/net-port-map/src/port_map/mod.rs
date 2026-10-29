use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use campfire_common::ErrorReport;

use campfire_threads::{Job, Worker};
use polling::Poller;
use tracing::{info, warn};

use crate::listening::TurnWait;
use crate::port_mapper::{MapRequest, MapTimes, PortMapper, Routes};
use crate::thread_stop::ThreadStop;

/// A server's port, which its router forwards from outside, on a thread of its own
/// (11-net-stack/port-map.md, A server maps its own port): it maps the port by PCP or NAT-PMP at
/// the default gateway, else by UPnP, renews the mapping, maps it anew when a renewal fails, and
/// tries again while no router maps it, in 30 minutes, when a PCP refusal's lifetime ends, or at
/// once when a router at an address not heard before announces itself, logging each change.
/// Dropped, it stops the thread's turn before its next request, has the thread delete the mapping,
/// and joins it.
#[derive(Debug)]
pub struct PortMap {
    stop: Arc<ThreadStop>,
    _worker: Worker,
}

impl PortMap {
    /// The map of the server socket at `local`, at this host's gateway or router, which a UPnP
    /// router lists as `name`, the app's; none, logged, for a socket no router forwards to, or
    /// when its thread cannot wait.
    pub fn start(local: SocketAddr, name: &'static str) -> Option<Self> {
        if !Self::is_forwardable(local) {
            info!(%local, "the server listens where no router forwards; its port is not mapped");
            return None;
        }
        let request = MapRequest {
            internal_port: local.port(),
            name,
        };
        let mapper = PortMapper::new(request, Routes::System, MapTimes::DEFAULT);
        match Self::of(mapper) {
            Ok(map) => Some(map),
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), "the port map's thread cannot wait; the server's port is not mapped");
                None
            }
        }
    }

    /// Whether a router forwards to a socket at `local`: one bound to IPv4, all of the host's
    /// addresses or one not loopback, or to the IPv6 wildcard, which takes IPv4 too as the stack
    /// binds it dual-stack (11-net-stack/port-map.md, IPv4 alone).
    const fn is_forwardable(local: SocketAddr) -> bool {
        match local.ip() {
            IpAddr::V4(ip) => !ip.is_loopback(),
            IpAddr::V6(ip) => ip.is_unspecified(),
        }
    }

    fn of(mapper: PortMapper) -> io::Result<Self> {
        let stop = Arc::new(ThreadStop {
            stopped: AtomicBool::new(false),
            poller: Poller::new()?,
        });
        let listening = TurnWait::open(Arc::clone(&stop), mapper.routes())?;
        let worker = Worker::start(Job::PortMap, move || listening.run(mapper));
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

#[cfg(test)]
mod tests;
