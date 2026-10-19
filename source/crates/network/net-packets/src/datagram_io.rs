use bytes::{Bytes, BytesMut};
use core::net::SocketAddr;
use core::time::Duration;

use crate::io_failed::IoFailed;

/// A datagram to send, and where.
#[derive(Debug)]
pub struct Outbound {
    pub to: SocketAddr,
    pub datagram: Bytes,
}

/// A datagram an IO received, and the address it came from.
#[derive(Debug, Clone)]
pub struct Datagram {
    pub from: SocketAddr,
    pub payload: BytesMut,
}

/// The datagram IO an endpoint drives: the emulated network's wires, or a socket. Its methods take
/// `&self`, as the IO thread reads and waits outside the endpoint's lock while the frame sends
/// (13-net-stack/packet-session.md, Two IO kinds: the socket and the emulated network).
pub trait DatagramIo: Send + Sync + core::fmt::Debug + 'static {
    /// Appends the datagrams that arrived to `out`, or fails for good: at least `max` of them
    /// when as many wait, and at most one read of the IO beyond `max`, so a caller that finds
    /// fewer than `max` appended read all that waited.
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed>;

    /// Sends each datagram of `outbound`, in order, and empties it: a socket joins the datagrams
    /// of one peer into one send where it can, through GSO.
    fn send(&self, outbound: &mut Vec<Outbound>);

    /// The address the IO sends from.
    fn local_address(&self) -> SocketAddr;

    /// The largest datagram the IO carries to `peer`.
    fn mtu(&self, peer: SocketAddr) -> usize;

    /// Blocks until a datagram may have arrived, `timeout` passed, or another thread called
    /// [`wake`](Self::wake).
    fn wait(&self, timeout: Option<Duration>);

    /// Ends the `wait` of another thread.
    fn wake(&self);
}
