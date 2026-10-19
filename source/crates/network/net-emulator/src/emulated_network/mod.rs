use crate::direction_counts::DirectionCounts;
use crate::duplex_model::DuplexModel;
use crate::emulated_direction::EmulatedDirection;
use crate::emulated_link_id::EmulatedLinkId;
use crate::phase::Phase;
use crate::side::Side;
use crate::wire_clock::WireClock;
use bevy_platform::time::Instant as OsInstant;
use bytes::Bytes;
use campfire_net_core::LinkDirection;
use core::net::IpAddr;
use core::net::Ipv4Addr;
use core::net::SocketAddr;
use core::task::Waker;
use core::time::Duration;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

/// An emulated network shared by the apps of one process: it makes [`EmulatedIo`] links and
/// schedules every delivery on its own wire clock (13-net-stack/testing.md, The emulated network is
/// the in-process IO, in `net-emulator`).
///
/// Clones share one network, so each app's `EmulatedIo` end and the harness that drives the wire
/// time hold the same state.
#[derive(Debug, Clone)]
pub struct EmulatedNetwork {
    inner: Arc<Mutex<EmulatedNetworkState>>,
}

#[derive(Debug)]
struct EmulatedNetworkState {
    wire: WireClock,
    origin: OsInstant,
    driven_now: Duration,
    links: Vec<EmulatedLink>,
}

#[derive(Debug)]
struct EmulatedLink {
    /// Indexed by [`LinkDirection::index`].
    directions: [EmulatedDirection; 2],
    /// Woken as a packet goes on the wire to each end, for an IO thread that waits for it.
    /// Indexed by [`Side::index`].
    wakers: [Option<Waker>; 2],
    mtu: Option<usize>,
}

impl EmulatedNetwork {
    pub fn new(wire: WireClock) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EmulatedNetworkState {
                wire,
                origin: OsInstant::now(),
                driven_now: Duration::ZERO,
                links: Vec::new(),
            })),
        }
    }

    /// A new link whose two directions follow `models`, as its two ends.
    pub fn pair(&self, models: DuplexModel) -> EmulatedPair {
        let mut state = self.lock();
        let link = EmulatedLinkId(state.links.len());
        state.links.push(EmulatedLink {
            directions: [
                EmulatedDirection::new(
                    models.client_to_server,
                    models.seeds[LinkDirection::ClientToServer.index()],
                ),
                EmulatedDirection::new(
                    models.server_to_client,
                    models.seeds[LinkDirection::ServerToClient.index()],
                ),
            ],
            wakers: [None, None],
            mtu: models.mtu,
        });
        drop(state);
        EmulatedPair {
            client: EmulatedIo::new(self.clone(), link, Side::Client),
            server: EmulatedIo::new(self.clone(), link, Side::Server),
        }
    }

    /// Sets the wire time of a [`WireClock::Driven`] network.
    pub fn set_time(&self, now: Duration) {
        let mut state = self.lock();
        assert_eq!(
            state.wire,
            WireClock::Driven,
            "only a driven wire clock is set"
        );
        debug_assert!(now >= state.driven_now, "the wire time does not go back");
        state.driven_now = now;
    }

    /// The wire time.
    pub fn now(&self) -> Duration {
        self.lock().now()
    }

    /// From `phase.start`, `direction` of `link` follows `phase`, until a later phase.
    pub fn add_phase(&self, link: EmulatedLinkId, direction: LinkDirection, phase: Phase) {
        self.lock().links[link.0].directions[direction.index()].add_phase(phase);
    }

    /// What `direction` of `link` did with its packets so far.
    pub fn counts(&self, link: EmulatedLinkId, direction: LinkDirection) -> DirectionCounts {
        self.lock().links[link.0].directions[direction.index()].counts
    }

    /// The MTU of `link`, if its model has one.
    pub(crate) fn mtu(&self, link: EmulatedLinkId) -> Option<usize> {
        self.lock().links[link.0].mtu
    }

    /// Hands `deliver` the datagrams due to each of `ends` by the wire time, with the end they
    /// reached, at most `max` in all, and returns how many: an IO of many ends reads them under
    /// one hold of the network.
    pub fn receive_ends(
        &self,
        ends: &[EmulatedIo],
        max: usize,
        mut deliver: impl FnMut(&EmulatedIo, Bytes),
    ) -> usize {
        let mut state = self.lock();
        let now = state.now();
        let mut left = max;
        for end in ends {
            debug_assert!(
                Arc::ptr_eq(&end.network.inner, &self.inner),
                "an end belongs to the network it reads from"
            );
            left -= state.links[end.link.0].directions[end.side.receives().index()].deliver_due(
                now,
                left,
                |bytes| deliver(end, bytes),
            );
        }
        max - left
    }

    /// The wire time until the next datagram in flight to any of `ends` is due, zero when one
    /// is due now, under one hold of the network.
    pub fn until_due_ends(&self, ends: &[EmulatedIo]) -> Option<Duration> {
        let state = self.lock();
        let now = state.now();
        ends.iter()
            .filter_map(|end| {
                state.links[end.link.0].directions[end.side.receives().index()].next_due()
            })
            .min()
            .map(|due| due.saturating_sub(now))
    }

    /// Puts `payloads` on the wire from `side`'s end of `link`, at the wire time, and wakes the
    /// peer's end when any went.
    pub(crate) fn send(
        &self,
        link: EmulatedLinkId,
        side: Side,
        payloads: impl Iterator<Item = Bytes>,
    ) {
        let mut state = self.lock();
        let now = state.now();
        let emulated_link = &mut state.links[link.0];
        let mtu = emulated_link.mtu;
        let direction = &mut emulated_link.directions[side.sends().index()];
        let mut sent = false;
        for payload in payloads {
            direction.send(payload, now, mtu);
            sent = true;
        }
        if sent && let Some(waker) = &emulated_link.wakers[side.peer().index()] {
            waker.wake_by_ref();
        }
    }

    /// Wakes `waker` as each packet goes on the wire to `side`'s end of `link`.
    pub(crate) fn set_waker(&self, link: EmulatedLinkId, side: Side, waker: Waker) {
        self.lock().links[link.0].wakers[side.index()] = Some(waker);
    }

    fn lock(&self) -> MutexGuard<'_, EmulatedNetworkState> {
        self.inner
            .lock()
            .expect("no emulated network user panicked")
    }
}

impl EmulatedNetworkState {
    fn now(&self) -> Duration {
        match self.wire {
            WireClock::Os => self.origin.elapsed(),
            WireClock::Driven => self.driven_now,
        }
    }
}

/// One end of a [`EmulatedNetwork`] link, which an IO holds.
///
/// [`EmulatedNetwork::pair`] makes both ends; one goes to a client's endpoint and the other to its
/// server's. The client end of each link has its own address, which the server end reports as its
/// peer's. The wire has no close and no timeout of its own: the protocol over it keeps both.
#[derive(Debug)]
pub struct EmulatedIo {
    pub(crate) network: EmulatedNetwork,
    pub(crate) link: EmulatedLinkId,
    pub(crate) side: Side,
}

impl EmulatedIo {
    /// The address of every link's server end.
    pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    pub(crate) const fn new(network: EmulatedNetwork, link: EmulatedLinkId, side: Side) -> Self {
        Self {
            network,
            link,
            side,
        }
    }

    /// The network this end belongs to.
    pub const fn network(&self) -> &EmulatedNetwork {
        &self.network
    }

    /// The link this end belongs to, which [`EmulatedNetwork::add_phase`] takes.
    pub const fn link(&self) -> EmulatedLinkId {
        self.link
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    /// The address of the link's client end, its own for each link.
    pub fn client_address(&self) -> SocketAddr {
        let port =
            u16::try_from(self.link.0 + 1).expect("an emulated network has at most 65,535 links");
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    /// Puts `datagrams` on the wire from this end.
    pub fn send(&self, datagrams: impl Iterator<Item = Bytes>) {
        self.network.send(self.link, self.side, datagrams);
    }

    /// The address of this end's peer: the server's for a client end, the client end's own for a
    /// server end.
    pub fn remote_address(&self) -> SocketAddr {
        match self.side {
            Side::Client => Self::SERVER_ADDR,
            Side::Server => self.client_address(),
        }
    }

    /// Wakes `waker` as each datagram goes on the wire to this end, for an IO thread that waits.
    pub fn set_waker(&self, waker: Waker) {
        self.network.set_waker(self.link, self.side, waker);
    }

    /// The link's MTU, if its model has one.
    pub fn mtu(&self) -> Option<usize> {
        self.network.mtu(self.link)
    }
}

/// The two ends of a new [`EmulatedNetwork`] link.
#[derive(Debug)]
pub struct EmulatedPair {
    pub client: EmulatedIo,
    pub server: EmulatedIo,
}

#[cfg(test)]
mod tests;
