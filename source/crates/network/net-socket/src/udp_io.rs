use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use campfire_common::ErrorReport;
use campfire_net_packets::{Datagram, DatagramIo, IoFailed, Outbound};
use campfire_net_payload::DEFAULT_MTU;
use polling::Events;
use quinn_udp::Transmit;
use tracing::{debug, trace, warn};

use crate::bind_error::BindError;
use crate::recv_batch::RecvBatch;
use crate::udp_port::UdpPort;

/// The socket of an endpoint: read in batches that GRO joins, written in sends that GSO joins for
/// one peer, and waited on by the IO thread with the poller (13-net-stack/packet-session.md, One
/// endpoint core and one pass for each IO).
#[derive(Debug)]
pub struct UdpIo {
    port: UdpPort,
    local: SocketAddr,
    batch: Mutex<RecvBatch>,
    events: Mutex<Events>,
    /// Where a GSO send's datagrams are joined.
    joined: Mutex<Vec<u8>>,
}

impl UdpIo {
    /// A socket bound on `address`.
    pub(crate) fn bind(address: SocketAddr) -> Result<Self, BindError> {
        let error = |error| BindError { address, error };
        let port = UdpPort::bind(address).map_err(error)?;
        let local = port.local_address().map_err(error)?;
        Ok(Self::new(port, local))
    }

    fn new(port: UdpPort, local: SocketAddr) -> Self {
        let batch = RecvBatch::new(&port, DEFAULT_MTU);
        Self {
            port,
            local,
            batch: Mutex::new(batch),
            events: Mutex::new(Events::new()),
            joined: Mutex::new(Vec::new()),
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .expect("no thread panics holding a socket's buffer")
    }

    /// Sends `contents` to `to` in datagrams of `segment` bytes, the last shorter at times. A send
    /// that fails drops its datagrams, which the packet sessions take as lost: a full socket, or a
    /// path that is down, which their timers find.
    fn transmit(&self, to: SocketAddr, contents: &[u8], segment: usize) {
        let transmit = Transmit {
            destination: to,
            ecn: None,
            contents,
            segment_size: (segment < contents.len()).then_some(segment),
            src_ip: None,
        };
        match self.port.send(&transmit) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                trace!("the socket is full; a send is dropped");
            }
            Err(error) => {
                debug!(error = %ErrorReport::of(&error), destination = %to, "a send is dropped");
            }
        }
    }
}

impl DatagramIo for UdpIo {
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
        let mut batch = Self::lock(&self.batch);
        let start = out.len();
        while out.len() - start < max {
            match batch.read(&self.port) {
                Ok(filled) => out.extend(batch.datagrams(filled)),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the socket failed");
                    return Err(IoFailed);
                }
            }
        }
        Ok(())
    }

    /// Joins each run of datagrams to one peer of one length, but its last, which may be shorter,
    /// into one GSO send of at most the socket's segments.
    fn send(&self, outbound: &mut Vec<Outbound>) {
        let max_segments = self.port.max_gso_segments().max(1);
        let mut joined = Self::lock(&self.joined);
        let mut run = 0;
        while run < outbound.len() {
            let Outbound { to, datagram } = &outbound[run];
            let segment = datagram.len();
            let mut end = run + 1;
            while end < outbound.len()
                && end - run < max_segments
                && outbound[end].to == *to
                && outbound[end - 1].datagram.len() == segment
                && outbound[end].datagram.len() <= segment
            {
                end += 1;
            }
            if end - run == 1 {
                self.transmit(*to, datagram, segment);
            } else {
                joined.clear();
                for outbound in &outbound[run..end] {
                    joined.extend_from_slice(&outbound.datagram);
                }
                self.transmit(*to, &joined, segment);
            }
            run = end;
        }
        outbound.clear();
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    fn local_address(&self) -> SocketAddr {
        self.local
    }

    fn mtu(&self, _peer: SocketAddr) -> usize {
        DEFAULT_MTU
    }

    fn wait(&self, timeout: Option<Duration>) {
        let mut events = Self::lock(&self.events);
        if let Err(error) = self.port.wait(&mut events, timeout) {
            warn!(error = %ErrorReport::of(&error), "the socket's wait failed");
        }
    }

    fn wake(&self) {
        self.port.wake();
    }
}
