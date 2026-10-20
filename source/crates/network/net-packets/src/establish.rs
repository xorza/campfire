use campfire_net_core::Instant;
use campfire_net_payload::UnlinkReason;
use core::net::SocketAddr;

use crate::connection_id::ConnectionId;
use crate::datagram_io::{Datagram, Outbound};
use crate::protection::Protection;
use crate::stateless_reset;
use campfire_common::ChannelBinding;

/// How an endpoint makes its packet sessions: at once and plain in-process, or by a handshake that
/// gives the seal over a network. The endpoint calls it with the datagrams that are not a
/// packet session's, and it hands back what it established. A client's establishment is a
/// [`ClientEstablish`] and a server's a [`ServerEstablish`], so neither holds the calls of the
/// other side (11-net-stack/api.md, A state the code cannot reach is a type).
pub trait Establish: Send + core::fmt::Debug + 'static {
    /// Takes a datagram whose first bit is the IO's own: a handshake's.
    fn receive(&mut self, now: Instant, datagram: Datagram, out: &mut Establishing) -> ReadCost;

    /// Runs the establishment's timers due at `now`.
    fn poll(&mut self, now: Instant, out: &mut Establishing);

    /// The earliest moment a timer of `poll` is due.
    fn next_deadline(&self) -> Option<Instant>;

    /// The packet session it established, routed by `route`, left the endpoint.
    fn ended(&mut self, route: Option<ConnectionId>);
}

/// A client's establishment, which opens the endpoint's one packet session.
pub trait ClientEstablish: Establish {
    /// Opens a packet session toward `server` at `now`.
    fn connect(&mut self, now: Instant, server: SocketAddr, out: &mut Establishing);
}

/// A server's establishment, which takes new peers while it accepts.
pub trait ServerEstablish: Establish {
    /// Takes a packet session datagram, its prefix still on, that no packet session of the endpoint
    /// holds: one that takes its peers by their first datagram while it accepts, or answers one it
    /// no longer knows with a stateless reset.
    fn unknown(&mut self, now: Instant, datagram: &Datagram, out: &mut Establishing);

    /// Whether it takes new peers, from the server's start to its stop.
    fn set_accepting(&mut self, accepting: bool);

    /// Its check of a peer's new path, when it routes each client's datagrams by the connection
    /// ID it gave, so a peer moves; none when it routes them by address, where no peer moves
    /// (11-net-stack/packet-session.md, A path check is a server's capability).
    fn path_check(&mut self) -> Option<&mut dyn PathCheck>;
}

/// A server's check of a peer's new path, which only an establishment that routes by connection ID
/// makes.
pub trait PathCheck {
    /// The 8 random bytes of a path challenge, which a peer on a spoofed address cannot guess.
    fn challenge(&mut self) -> [u8; 8];
}

/// What an establishment's read of one datagram cost the endpoint's hold of its lock, which ends
/// after a handshake's start (11-net-stack/packet-session.md, A hold starts at most one handshake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadCost {
    /// It started a handshake, whose first flight costs as much as a batch of datagrams.
    Handshake,
    /// Less: a Retry, a refusal, a step or the end of a handshake, or nothing.
    Light,
}

/// What an establishment hands its endpoint back.
#[derive(Debug, Default)]
pub struct Establishing {
    /// Datagrams of its own, as a handshake's.
    pub outbound: Vec<Outbound>,
    pub established: Vec<Established>,
    /// A client's connect failed for this reason.
    pub failed: Option<UnlinkReason>,
}

/// A packet session the establishment made with `peer`.
#[derive(Debug)]
pub struct Established {
    pub peer: SocketAddr,
    pub protection: Protection,
    /// The connection ID each datagram this end sends starts with, which the peer's IO routes it
    /// by.
    pub prefix: Option<ConnectionId>,
    /// The connection ID that starts each of the peer's datagrams to this end, which routes them.
    pub route: Option<ConnectionId>,
    /// The token of the peer's stateless reset.
    pub reset_token: Option<[u8; stateless_reset::TOKEN_LEN]>,
    /// The binding of the TLS packet session that made the packet session.
    pub binding: Option<ChannelBinding>,
}
