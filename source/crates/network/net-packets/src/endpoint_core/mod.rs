//! An endpoint's sessions and its establishment, which its pass drives with the IO's datagrams
//! and its timers, and which the frame reads and feeds through each link's queues.

use bytes::Buf;
use core::cmp::Reverse;
use core::net::SocketAddr;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

use bevy_platform::time::Instant as OsInstant;
use campfire_net_core::Instant;
use campfire_net_emulator::LinkConditioner;
use campfire_net_payload::{AppCode, SessionQueues, UnlinkReason};

use crate::carrier::Carrier;
use crate::channel_binding::ChannelBinding;
use crate::connection_id::ConnectionId;
use crate::datagram_io::Datagram;
use crate::datagram_io::Outbound;
use crate::endpoint_parts::EndpointSide;
use crate::establish::{ClientEstablish, Establish, Established, Establishing, ServerEstablish};
use crate::header::IO_BIT;
use crate::packet_session::{PacketSession, PathOutcome, Received};
use crate::session_config::SessionConfig;
use crate::stateless_reset;
use crate::transmit::{Path, Transmit};

/// A session's slot in its endpoint's table, and its id, which no later session in the slot
/// shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PeerKey {
    slot: usize,
    id: u64,
}

/// What routes a datagram to its session: the connection ID it starts with, or, on an endpoint
/// that routes by address, the address it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RouteKey {
    Id(ConnectionId),
    Address(SocketAddr),
}

/// A session's entry in its endpoint's timers: when its pass polls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Timer {
    at: Instant,
    key: PeerKey,
}

/// What the pass tells the endpoint's host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum News<C> {
    /// A session opened with the peer at `address`, its TLS session's binding if it has one.
    Linked {
        key: PeerKey,
        address: SocketAddr,
        binding: Option<ChannelBinding>,
    },
    /// The session that `carrier` carries ended, as `reason` says, or the client's connect that
    /// it carries failed.
    Ended { carrier: C, reason: UnlinkReason },
}

/// Whether the endpoint opens one session to its server or takes sessions, with the
/// establishment of that side.
#[derive(Debug)]
enum Side<C> {
    /// A client's endpoint, whose session the host's own carrier carries from its start.
    Client {
        server: SocketAddr,
        carrier: Option<C>,
        establish: Box<dyn ClientEstablish>,
    },
    /// A server's endpoint, whose establishment takes new peers while it accepts.
    Server { establish: Box<dyn ServerEstablish> },
}

impl<C> Side<C> {
    /// The establishment, in the calls both sides take.
    fn establish(&mut self) -> &mut dyn Establish {
        match self {
            Self::Client { establish, .. } => establish.as_mut(),
            Self::Server { establish } => establish.as_mut(),
        }
    }

    fn establish_ref(&self) -> &dyn Establish {
        match self {
            Self::Client { establish, .. } => establish.as_ref(),
            Self::Server { establish } => establish.as_ref(),
        }
    }
}

/// The time the endpoint's sessions read: the frame's, as the frame sets it, on a driven clock;
/// the OS's since the app's origin on a real one, so the IO thread reads the app's time.
#[derive(Debug, Clone, Copy)]
pub enum EndpointTime {
    Driven(Instant),
    Os(OsInstant),
}

impl EndpointTime {
    pub fn now(self) -> Instant {
        match self {
            Self::Driven(now) => now,
            Self::Os(origin) => Instant::from_elapsed(origin.elapsed()),
        }
    }
}

#[derive(Debug)]
struct Peer<C> {
    session: PacketSession,
    key: PeerKey,
    /// The token of the peer's stateless reset, which ends the session with no packet it opens.
    reset_token: Option<[u8; stateless_reset::TOKEN_LEN]>,
    address: SocketAddr,
    /// A new address the peer's packets came from, which its session checks.
    candidate: Option<SocketAddr>,
    route: Option<ConnectionId>,
    /// What carries the session for the host, once the host gave it one.
    carrier: Option<C>,
    /// The largest datagram the IO carries to the peer, which the host names as it carries the
    /// session: a peer's MTU holds for its session's life.
    mtu: usize,
    /// When the session's entry in the timers is due, at or before its next deadline; none while
    /// the pass runs it. An entry of another time is stale.
    scheduled: Option<Instant>,
}

impl<C> Peer<C> {
    fn route_key(&self) -> RouteKey {
        self.route
            .map_or(RouteKey::Address(self.address), RouteKey::Id)
    }
}

/// An endpoint's sessions and its establishment, which a host drives through an
/// `EndpointRuntime`, and whose sessions each
/// carrier `C` of the host carries.
#[derive(Debug)]
pub struct EndpointCore<C> {
    config: SessionConfig,
    side: Side<C>,
    peers: Vec<Option<Peer<C>>>,
    /// The empty slots of `peers`, which a new session takes before the vector grows.
    free: Vec<usize>,
    /// The slot of each session by what routes its datagrams (13-net-stack/session.md, A pass
    /// touches only the sessions that need it).
    routes: BTreeMap<RouteKey, usize>,
    /// Each session's next deadline, so a pass polls only the sessions that are due.
    timers: BinaryHeap<Reverse<Timer>>,
    /// The timers a pass takes due before it polls their sessions.
    due: Vec<Timer>,
    next_id: u64,
    news: VecDeque<News<C>>,
    /// The model the endpoint applies to each datagram the IO gives, before the sessions see it.
    conditioner: Option<LinkConditioner<Datagram>>,
    establishing: Establishing,
    transmits: Vec<Transmit>,
    time: EndpointTime,
    /// Whether the endpoint dropped: its pass ends once its closes went.
    ended: bool,
}

impl<C: Carrier> EndpointCore<C> {
    pub(crate) fn new(
        config: SessionConfig,
        side: EndpointSide,
        conditioner: Option<LinkConditioner<Datagram>>,
    ) -> Self {
        assert!(
            config.keep_alive < config.idle_timeout,
            "a keep-alive less often than the idle timeout lets a quiet session time out"
        );
        Self {
            config,
            side: match side {
                EndpointSide::Client { server, establish } => Side::Client {
                    server,
                    carrier: None,
                    establish,
                },
                EndpointSide::Server { establish } => Side::Server { establish },
            },
            peers: Vec::new(),
            free: Vec::new(),
            routes: BTreeMap::new(),
            timers: BinaryHeap::new(),
            due: Vec::new(),
            next_id: 0,
            news: VecDeque::new(),
            conditioner,
            establishing: Establishing::default(),
            transmits: Vec::new(),
            time: EndpointTime::Driven(Instant::ZERO),
            ended: false,
        }
    }

    pub(crate) const fn ended(&self) -> bool {
        self.ended
    }

    pub const fn is_client(&self) -> bool {
        matches!(self.side, Side::Client { .. })
    }

    pub fn now(&self) -> Instant {
        self.time.now()
    }

    pub const fn set_time(&mut self, time: EndpointTime) {
        self.time = time;
    }

    /// Starts the endpoint: a client connects to its server, its session carried by `carrier`,
    /// and a server accepts.
    pub fn start(&mut self, carrier: C, out: &mut Vec<Outbound>) {
        let now = self.now();
        match &mut self.side {
            Side::Client {
                server,
                carrier: client,
                establish,
            } => {
                *client = Some(carrier);
                // A client that connects again drops its last session, closing or not: the new
                // one's datagrams come from the same address.
                self.free.clear();
                for peer in self.peers.drain(..).flatten() {
                    establish.ended(peer.route);
                }
                self.routes.clear();
                self.timers.clear();
                establish.connect(now, *server, &mut self.establishing);
            }
            Side::Server { establish } => establish.set_accepting(true),
        }
        self.apply_establishing(now, out);
    }

    /// Takes no new peer, as a server that stopped.
    pub fn stop_accepting(&mut self) {
        if let Side::Server { establish } = &mut self.side {
            establish.set_accepting(false);
        }
    }

    /// One pass over `datagrams` the IO gave, then the timers: what goes out is appended to
    /// `out`.
    pub(crate) fn pass(&mut self, datagrams: &mut Vec<Datagram>, out: &mut Vec<Outbound>) {
        let now = self.now();
        if let Some(conditioner) = &mut self.conditioner {
            for datagram in datagrams.drain(..) {
                let bytes = datagram.payload.len();
                conditioner.condition_packet(datagram, bytes, now);
            }
            while let Some(datagram) = conditioner.pop_packet(now) {
                datagrams.push(datagram);
            }
        }
        for datagram in datagrams.drain(..) {
            self.route(now, datagram, out);
        }
        self.side.establish().poll(now, &mut self.establishing);
        self.apply_establishing(now, out);
        // The due timers are taken first, so a pass polls each session once, as a timer it
        // leaves due waits for the next pass.
        self.due.clear();
        while let Some(&Reverse(timer)) = self.timers.peek()
            && timer.at <= now
        {
            self.timers.pop();
            self.due.push(timer);
        }
        for &Timer { at, key } in &self.due {
            let Some(slot) = self.peers.get_mut(key.slot) else {
                continue;
            };
            let Some(peer) = slot
                .as_mut()
                .filter(|peer| peer.key == key && peer.scheduled == Some(at))
            else {
                continue;
            };
            peer.scheduled = None;
            peer.session.poll(now, &mut self.transmits);
            Self::check_path(peer);
            Self::flush(peer, &mut self.transmits, out);
            if let Some(reason) = peer.session.ended() {
                if let Some(carrier) = peer.carrier {
                    self.news.push_back(News::Ended { carrier, reason });
                }
                self.side.establish().ended(peer.route);
                self.routes.remove(&peer.route_key());
                *slot = None;
                self.free.push(key.slot);
            } else {
                Self::schedule(peer, &mut self.timers, now);
            }
        }
        debug_assert!(
            self.peers.iter().flatten().all(|peer| peer
                .scheduled
                .is_some_and(|at| peer.session.next_deadline().is_none_or(|due| at <= due))),
            "each session's timer is due at or before its next deadline"
        );
    }

    /// Enters `peer`'s next deadline in `timers` when it comes before its entry: a session that
    /// ended is due at `now`, which removes it.
    fn schedule(peer: &mut Peer<C>, timers: &mut BinaryHeap<Reverse<Timer>>, now: Instant) {
        let due = peer.session.next_deadline().unwrap_or(now);
        if peer.scheduled.is_none_or(|at| due < at) {
            peer.scheduled = Some(due);
            timers.push(Reverse(Timer {
                at: due,
                key: peer.key,
            }));
        }
    }

    fn route(&mut self, now: Instant, datagram: Datagram, out: &mut Vec<Outbound>) {
        let Some(&first) = datagram.payload.first() else {
            return;
        };
        if first & IO_BIT != 0 {
            self.side
                .establish()
                .receive(now, datagram, &mut self.establishing);
            self.apply_establishing(now, out);
            return;
        }
        let from = datagram.from;
        let route = if self.side.establish_ref().routes_by_prefix() {
            let Some(id) = datagram
                .payload
                .first_chunk()
                .and_then(|bytes| ConnectionId::from_bytes(*bytes))
            else {
                return;
            };
            Some(id)
        } else {
            None
        };
        let route_key = route.map_or(RouteKey::Address(from), RouteKey::Id);
        let slot = if let Some(&slot) = self.routes.get(&route_key) {
            slot
        } else {
            let Side::Server { establish } = &mut self.side else {
                return;
            };
            establish.unknown(now, &datagram, &mut self.establishing);
            self.apply_establishing(now, out);
            let Some(&slot) = self.routes.get(&route_key) else {
                return;
            };
            slot
        };
        let mut payload = datagram.payload;
        if route.is_some() {
            payload.advance(ConnectionId::LEN);
        }
        let peer = self.peers[slot]
            .as_mut()
            .expect("a slot that routes holds a peer");
        if peer
            .reset_token
            .is_some_and(|token| stateless_reset::matches(&payload, &token))
        {
            peer.session.reset();
            Self::schedule(peer, &mut self.timers, now);
            return;
        }
        let path = if from == peer.address {
            Path::Peer
        } else {
            Path::Candidate
        };
        let received = peer
            .session
            .receive(now, payload, path, &mut self.transmits);
        // A peer routed by its ID whose newest packet comes from an address of its own is
        // checked there before its datagrams go to it.
        if path == Path::Candidate
            && peer.candidate != Some(from)
            && received == (Received::Read { largest: true })
        {
            peer.candidate = Some(from);
            let data = self.side.establish().challenge();
            peer.session.start_challenge(now, data, &mut self.transmits);
        }
        Self::check_path(peer);
        Self::flush(peer, &mut self.transmits, out);
        Self::schedule(peer, &mut self.timers, now);
    }

    /// Hands the establishment's output on: its datagrams go out, each session it made takes a
    /// slot, and a failed connect ends the client's carrier.
    fn apply_establishing(&mut self, now: Instant, out: &mut Vec<Outbound>) {
        out.append(&mut self.establishing.outbound);
        for established in core::mem::take(&mut self.establishing.established) {
            self.open(now, established, out);
        }
        if let Some(reason) = self.establishing.failed.take()
            && let Side::Client {
                carrier: Some(carrier),
                ..
            } = &self.side
        {
            let carrier = *carrier;
            self.news.push_back(News::Ended { carrier, reason });
        }
    }

    fn open(&mut self, now: Instant, established: Established, out: &mut Vec<Outbound>) {
        let Established {
            peer,
            protection,
            prefix,
            route,
            reset_token,
            binding,
        } = established;
        let slot = self.free.pop().unwrap_or_else(|| {
            self.peers.push(None);
            self.peers.len() - 1
        });
        let key = PeerKey {
            slot,
            id: self.next_id,
        };
        self.next_id += 1;
        let mut session = PacketSession::new(protection, prefix, self.config, now);
        // Each end's first packet tells its peer the session is open: a client's server spawns
        // its link, and a server's client stops sending its handshake's end.
        session.ping(now, &mut self.transmits);
        let carrier = match &self.side {
            Side::Client { carrier, .. } => *carrier,
            Side::Server { .. } => None,
        };
        let mut peer = Peer {
            session,
            key,
            reset_token,
            address: peer,
            candidate: None,
            route,
            carrier,
            mtu: 0,
            scheduled: None,
        };
        Self::flush(&peer, &mut self.transmits, out);
        Self::schedule(&mut peer, &mut self.timers, now);
        self.news.push_back(News::Linked {
            key,
            address: peer.address,
            binding,
        });
        let replaced = self.routes.insert(peer.route_key(), slot);
        debug_assert!(replaced.is_none(), "one session holds each route");
        self.peers[slot] = Some(peer);
    }

    /// Takes what the peer's path check came to: an answered candidate is its address now, and a
    /// silent one goes.
    const fn check_path(peer: &mut Peer<C>) {
        match peer.session.take_path_outcome() {
            Some(PathOutcome::Validated) => {
                if let Some(candidate) = peer.candidate.take() {
                    peer.address = candidate;
                }
            }
            Some(PathOutcome::Failed) => peer.candidate = None,
            None => {}
        }
    }

    /// Moves a session's transmits to `out`, each to the address of its path.
    fn flush(peer: &Peer<C>, transmits: &mut Vec<Transmit>, out: &mut Vec<Outbound>) {
        for Transmit { path, datagram } in transmits.drain(..) {
            let to = match path {
                Path::Peer => peer.address,
                Path::Candidate => peer.candidate.unwrap_or(peer.address),
            };
            out.push(Outbound {
                to,
                datagram: datagram.freeze(),
            });
        }
    }

    /// The oldest news the host has not taken.
    pub fn next_news(&mut self) -> Option<News<C>> {
        self.news.pop_front()
    }

    /// Records that `carrier` carries the session `key` names, whose datagrams the IO carries
    /// at most `mtu` bytes.
    pub fn carry(&mut self, key: PeerKey, carrier: C, mtu: usize) {
        if let Some(peer) = self.peer(key) {
            peer.carrier = Some(carrier);
            peer.mtu = mtu;
        }
    }

    fn peer(&mut self, key: PeerKey) -> Option<&mut Peer<C>> {
        self.peers
            .get_mut(key.slot)
            .and_then(Option::as_mut)
            .filter(|peer| peer.key == key)
    }

    /// Each session a carrier carries, with its carrier.
    pub fn carried(&self) -> impl Iterator<Item = (PeerKey, C)> + '_ {
        self.peers
            .iter()
            .flatten()
            .filter_map(|peer| peer.carrier.map(|carrier| (peer.key, carrier)))
    }

    /// Hands the host's queues of session `key` what its session has for them, its datagrams
    /// at most the MTU its carry named.
    pub fn hand_over(&mut self, key: PeerKey, queues: &mut SessionQueues) {
        let now = self.now();
        if let Some(peer) = self.peer(key) {
            debug_assert!(
                peer.carrier.is_some(),
                "a host hands over a session it carries"
            );
            peer.session.hand_over(queues, peer.mtu, now);
        }
    }

    /// Takes what the host's queues of session `key` give, and appends what goes out to `out`.
    pub fn take_over(&mut self, key: PeerKey, queues: &mut SessionQueues, out: &mut Vec<Outbound>) {
        let now = self.now();
        let Some(slot) = self.peers.get_mut(key.slot) else {
            return;
        };
        let Some(peer) = slot.as_mut().filter(|peer| peer.key == key) else {
            return;
        };
        peer.session.take_over(queues, now, &mut self.transmits);
        Self::flush(peer, &mut self.transmits, out);
        Self::schedule(peer, &mut self.timers, now);
    }

    /// Closes session `key` with `code`, which then answers its peer for a while with no
    /// carrier: a link its own end closed has no loss to tell.
    pub(crate) fn close(&mut self, key: PeerKey, code: AppCode, out: &mut Vec<Outbound>) {
        let now = self.now();
        let Some(slot) = self.peers.get_mut(key.slot) else {
            return;
        };
        let Some(peer) = slot.as_mut().filter(|peer| peer.key == key) else {
            return;
        };
        peer.carrier = None;
        peer.session.close(now, code, &mut self.transmits);
        Self::flush(peer, &mut self.transmits, out);
        Self::schedule(peer, &mut self.timers, now);
    }

    /// Ends every session as the IO failed: each carrier of one learns it, and the pass ends.
    pub(crate) fn fail(&mut self) {
        self.ended = true;
        self.stop_accepting();
        self.free.clear();
        for peer in self.peers.drain(..).flatten() {
            if let Some(carrier) = peer.carrier {
                self.news.push_back(News::Ended {
                    carrier,
                    reason: UnlinkReason::SocketFailed,
                });
            }
        }
        self.routes.clear();
        self.timers.clear();
    }

    /// Closes every session with `AppCode::DROPPED`, takes no new one, and lets the pass end.
    pub fn close_all(&mut self, out: &mut Vec<Outbound>) {
        let now = self.now();
        self.ended = true;
        self.stop_accepting();
        for peer in self.peers.iter_mut().flatten() {
            peer.carrier = None;
            peer.session
                .close(now, AppCode::DROPPED, &mut self.transmits);
            Self::flush(peer, &mut self.transmits, out);
            Self::schedule(peer, &mut self.timers, now);
        }
    }

    /// The earliest moment the pass has work: a session's timer, the establishment's, or a
    /// datagram the conditioner holds. A stale timer may come first, and then wakes a pass
    /// that finds nothing to do.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.timers
            .peek()
            .map(|Reverse(timer)| timer.at)
            .into_iter()
            .chain(self.side.establish_ref().next_deadline())
            .chain(
                self.conditioner
                    .as_ref()
                    .and_then(LinkConditioner::next_due),
            )
            .min()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::carrier::Carrier;
    use crate::endpoint_core::EndpointCore;
    use crate::packet_session::internals::SessionCounts;

    impl<C: Carrier> EndpointCore<C> {
        /// The counts of the session that `carrier` carries.
        pub fn session_counts(&self, carrier: C) -> Option<SessionCounts> {
            self.peers
                .iter()
                .flatten()
                .find(|peer| peer.carrier == Some(carrier))
                .map(|peer| peer.session.counts())
        }
    }
}

#[cfg(test)]
mod tests;
