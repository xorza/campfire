use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
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

/// The most bytes one UDP send carries over IPv4, 65,535 less its IP and UDP headers: Linux refuses
/// a GSO send past it with `EMSGSIZE` (RFC 768; Linux `udp_sendmsg`).
const MAX_SEND: usize = 65_507;

/// The reads in a row that fail before the IO does: one datagram's error fails one read.
const FAILED_READS: u32 = 64;

/// The socket's receive buffer, and how many reads in a row failed.
#[derive(Debug)]
struct Reads {
    batch: RecvBatch,
    failed: u32,
}

/// The socket of an endpoint: read in batches that GRO joins, written in sends that GSO joins for
/// one peer, and waited on by the IO thread with the poller (11-net-stack/packet-session.md, One
/// endpoint core and one pass for each IO).
#[derive(Debug)]
pub struct UdpIo {
    port: UdpPort,
    local: SocketAddr,
    batch: Mutex<Reads>,
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
            batch: Mutex::new(Reads { batch, failed: 0 }),
            events: Mutex::new(Events::new()),
            joined: Mutex::new(Vec::new()),
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .expect("no thread panics holding a socket's buffer")
    }

    /// Sends `contents` to `to` from `from_ip`, or the OS's choice, in datagrams of `segment`
    /// bytes, the last shorter at times. A send that fails drops its datagrams, which the packet
    /// sessions take as lost: a full socket, or a path that is down, which their timers find.
    fn transmit(&self, to: SocketAddr, from_ip: Option<IpAddr>, contents: &[u8], segment: usize) {
        let transmit = Transmit {
            destination: to,
            ecn: None,
            contents,
            segment_size: (segment < contents.len()).then_some(segment),
            src_ip: from_ip,
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
    /// Reads what waits. A read error is one datagram's, an ICMP report a later read tells or a
    /// full buffer, which std names no kind of at times, as Windows' `WSAENETRESET` and Linux's
    /// `ENOBUFS`: the read is dropped, and the IO fails only once `FAILED_READS` reads in a row
    /// fail, as a socket that broke fails every read.
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
        let mut reads = Self::lock(&self.batch);
        let Reads { batch, failed } = &mut *reads;
        let start = out.len();
        while out.len() - start < max {
            match batch.read(&self.port) {
                Ok(filled) => {
                    *failed = 0;
                    out.extend(batch.datagrams(filled));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => {
                    *failed += 1;
                    if *failed >= FAILED_READS {
                        warn!(error = %ErrorReport::of(&error), "the socket failed");
                        return Err(IoFailed);
                    }
                    debug!(error = %ErrorReport::of(&error), "a read is dropped");
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Joins each run of datagrams to one peer from one local address, of one length but its
    /// last, which may be shorter, into one GSO send of at most the socket's segments and at most
    /// the bytes one UDP datagram carries, which the OS refuses a send past whole.
    fn send(&self, outbound: &mut Vec<Outbound>) {
        let max_segments = self.port.max_gso_segments().max(1);
        let mut joined = Self::lock(&self.joined);
        let mut run = 0;
        while run < outbound.len() {
            let Outbound {
                to,
                from_ip,
                datagram,
            } = &outbound[run];
            let segment = datagram.len();
            let segments = max_segments.min(MAX_SEND / segment.max(1)).max(1);
            let mut end = run + 1;
            while end < outbound.len()
                && end - run < segments
                && outbound[end].to == *to
                && outbound[end].from_ip == *from_ip
                && outbound[end - 1].datagram.len() == segment
                && outbound[end].datagram.len() <= segment
            {
                end += 1;
            }
            if end - run == 1 {
                self.transmit(*to, *from_ip, datagram, segment);
            } else {
                joined.clear();
                for outbound in &outbound[run..end] {
                    joined.extend_from_slice(&outbound.datagram);
                }
                self.transmit(*to, *from_ip, &joined, segment);
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
