//! `atomic/endpoint/…`: the holds of a UDP server's endpoint lock, for which its frame or its IO
//! thread waits, each case the longest hold of one step: a pass over a full batch of its links'
//! datagrams, a pass that sends each link its keep-alive, a frame's own steps over its links, and
//! a pass over a full batch of handshakes' starts, and one over their ends.

use core::time::Duration;

use campfire_measure::{Kind, Scale};
use campfire_net_core::{Instant, LinkDirection};
use campfire_net_emulator::{
    DirectionCounts, DuplexModel, EmulatedLinkId, EmulatedNetwork, WireClock,
};
use campfire_net_packets::EmulatedWires;
use campfire_net_packets::internals::{CLIENT, HOLD_BATCH, HoldClock, PlainHost};
use criterion::{BenchmarkId, Criterion, Throughput};

use crate::test_link::TestLink;

/// The links of each case of a server with links.
const LINKS: [usize; 4] = [64, 256, 1024, 4096];

/// The time between two frames.
const FRAME: Duration = Duration::from_millis(10);

/// The bytes of each payload a client sends: a few orders.
const PAYLOAD: [u8; 64] = [0; 64];

/// The most frames a server and its clients take to link.
const LINK_FRAMES: usize = 100;

/// How long after its keep-alive's ping each client answers: past the ACK's delay.
const ANSWER: Duration = Duration::from_millis(100);

pub(crate) fn endpoint(c: &mut Criterion) {
    let mut group = Kind::Atomic.group(c, "endpoint", Scale::Frame);
    for links in LINKS {
        let mut load = ServerLoad::linked(links);
        let name = campfire_measure::count(links, "link");
        group.throughput(Throughput::Elements(u64::try_from(load.batch()).unwrap()));
        group.bench_function(
            BenchmarkId::from_parameter(format!("datagrams_{name}")),
            |b| {
                b.iter_custom(|iters| (0..iters).map(|_| load.datagrams()).sum());
            },
        );
        group.throughput(Throughput::Elements(u64::try_from(links).unwrap()));
        group.bench_function(
            BenchmarkId::from_parameter(format!("keep_alives_{name}")),
            |b| {
                b.iter_custom(|iters| (0..iters).map(|_| load.keep_alives()).sum());
            },
        );
        group.bench_function(BenchmarkId::from_parameter(format!("frame_{name}")), |b| {
            b.iter_custom(|iters| (0..iters).map(|_| load.frame()).sum());
        });
    }
    group.throughput(Throughput::Elements(u64::try_from(HOLD_BATCH).unwrap()));
    group.bench_function(BenchmarkId::from_parameter("handshakes"), |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| ServerLoad::retried(HOLD_BATCH).handshakes())
                .sum()
        });
    });
    group.bench_function(BenchmarkId::from_parameter("handshake_ends"), |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| ServerLoad::flown(HOLD_BATCH).handshake_ends())
                .sum()
        });
    });
    group.finish();
}

/// A server and its clients on the emulated network, each client on a link of its own, stepped
/// by hand on its wire time; and the clock of the server's holds of its endpoint's lock.
#[derive(Debug)]
struct ServerLoad {
    network: EmulatedNetwork,
    server: PlainHost<EmulatedWires>,
    clients: Vec<PlainHost<EmulatedWires>>,
    links: Vec<EmulatedLinkId>,
    now: Duration,
    /// The client that sends first in the next batch, as the clients take turns.
    next: usize,
    clock: HoldClock,
}

impl ServerLoad {
    /// A server and `links` clients that each sent it their first `Initial`.
    fn started(links: usize) -> Self {
        let network = EmulatedNetwork::new(WireClock::Driven);
        let mut server =
            PlainHost::driven(TestLink::server(TestLink::setup()).emulated(TestLink::roomy(), 0));
        server.start(Instant::ZERO);
        let mut clients = Vec::with_capacity(links);
        let mut ids = Vec::with_capacity(links);
        for seed in 1..=u64::try_from(links).unwrap() {
            let pair = network.pair(DuplexModel::default().with_seed(seed));
            ids.push(pair.server.link());
            server.io().add(pair.server);
            let client = TestLink::client(TestLink::setup()).emulated(pair.client, seed);
            let mut client = PlainHost::driven(client);
            client.start(Instant::ZERO);
            clients.push(client);
        }
        Self {
            network,
            server,
            clients,
            links: ids,
            now: Duration::ZERO,
            next: 0,
            clock: HoldClock::default(),
        }
    }

    /// A server and `links` clients linked, quiet since each client's last answer.
    fn linked(links: usize) -> Self {
        let mut load = Self::started(links);
        for _ in 0..LINK_FRAMES {
            if load.server.linked().count() == links
                && load
                    .clients
                    .iter()
                    .all(|client| client.link(CLIENT).is_some_and(|link| link.linked))
            {
                load.keep_alives();
                return load;
            }
            load.advance(FRAME);
            load.frames();
        }
        panic!("{links} clients link within {LINK_FRAMES} frames");
    }

    /// A server and `links` clients each about to send their `Initial` again with the token of
    /// the server's Retry.
    fn retried(links: usize) -> Self {
        let mut load = Self::started(links);
        let now = load.instant();
        load.server.frame(now);
        for client in &mut load.clients {
            client.frame(now);
        }
        load
    }

    /// The datagrams a batch holds: one from each client, at most a pass's batch.
    fn batch(&self) -> usize {
        self.clients.len().min(HOLD_BATCH)
    }

    /// The next batch of clients each send the server one payload a frame after the last batch,
    /// and the server reads them: the longest hold of that pass, then the server's frame.
    fn datagrams(&mut self) -> Duration {
        self.advance(FRAME);
        let now = self.instant();
        let batch = self.batch();
        for _ in 0..batch {
            let client = &mut self.clients[self.next];
            client.queue(CLIENT, 0, &PAYLOAD);
            client.frame(now);
            self.next = (self.next + 1) % self.clients.len();
        }
        let delivered = self.counts(LinkDirection::ClientToServer, |counts| counts.delivered);
        let longest = self.timed_pass();
        debug_assert_eq!(
            self.counts(LinkDirection::ClientToServer, |counts| counts.delivered) - delivered,
            u64::try_from(batch).unwrap(),
            "the pass reads one datagram from each client of the batch"
        );
        self.server.frame(now);
        longest
    }

    /// The server sends each link its keep-alive, a `Ping`, as it sent nothing to any for as
    /// long: the longest hold of that pass. Each client then answers with its ACK, which the
    /// server reads.
    fn keep_alives(&mut self) -> Duration {
        let keep_alive = TestLink::setup().packet_session.keep_alive;
        self.advance(keep_alive - ANSWER);
        let sent = self.counts(LinkDirection::ServerToClient, |counts| counts.sent);
        let longest = self.timed_pass();
        debug_assert_eq!(
            self.counts(LinkDirection::ServerToClient, |counts| counts.sent) - sent,
            u64::try_from(self.clients.len()).unwrap(),
            "the pass sends each client one keep-alive"
        );
        let now = self.instant();
        for client in &mut self.clients {
            client.frame(now);
        }
        self.advance(ANSWER);
        self.frames();
        longest
    }

    /// The server's frame at the instant of its last, with nothing new: the longest of its holds,
    /// its two steps over every link it carries.
    fn frame(&mut self) -> Duration {
        let sent = self.counts(LinkDirection::ServerToClient, |counts| counts.sent);
        self.clock.clear();
        self.server.timed_frame(self.instant(), &mut self.clock);
        debug_assert_eq!(
            self.counts(LinkDirection::ServerToClient, |counts| counts.sent),
            sent,
            "a frame with nothing new sends nothing"
        );
        self.longest()
    }

    /// The server reads each client's `Initial` with its token, and starts its handshake: the
    /// longest hold of that pass.
    fn handshakes(&mut self) -> Duration {
        let sent = self.counts(LinkDirection::ServerToClient, |counts| counts.sent);
        let longest = self.timed_pass();
        debug_assert_eq!(
            self.counts(LinkDirection::ServerToClient, |counts| counts.sent) - sent,
            2 * u64::try_from(self.clients.len()).unwrap(),
            "the pass sends each client the server's flight, in two datagrams"
        );
        longest
    }

    /// A server and `links` clients each about to send the end of its handshake.
    fn flown(links: usize) -> Self {
        let mut load = Self::retried(links);
        load.handshakes();
        let now = load.instant();
        for client in &mut load.clients {
            client.frame(now);
        }
        load
    }

    /// The server reads the end of each client's handshake, and opens its packet session: the
    /// longest hold of that pass.
    fn handshake_ends(&mut self) -> Duration {
        let longest = self.timed_pass();
        debug_assert_eq!(
            self.server.linked().count(),
            0,
            "the host links each packet session in its frame"
        );
        longest
    }

    const fn instant(&self) -> Instant {
        Instant::from_elapsed(self.now)
    }

    fn advance(&mut self, by: Duration) {
        self.now += by;
        self.network.set_time(self.now);
    }

    /// Each client's frame, then the server's.
    fn frames(&mut self) {
        let now = self.instant();
        for client in &mut self.clients {
            client.frame(now);
        }
        self.server.frame(now);
    }

    /// The server's pass alone: the longest of its holds.
    fn timed_pass(&mut self) -> Duration {
        self.clock.clear();
        self.server.timed_pass(self.instant(), &mut self.clock);
        self.longest()
    }

    fn longest(&self) -> Duration {
        self.clock.longest().expect("a step holds the lock")
    }

    /// The sum over every link of `count` of its `direction`.
    fn counts(&self, direction: LinkDirection, count: impl Fn(DirectionCounts) -> u64) -> u64 {
        self.links
            .iter()
            .map(|&link| count(self.network.counts(link, direction)))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each step of a server of a few links does what its case measures, as the step's own
    /// checks hold: a batch of one datagram from each client, one keep-alive to each, a frame
    /// that sends nothing, and the server's flight to each client of a full batch.
    #[test]
    fn each_step_makes_the_holds_its_case_measures() {
        let mut load = ServerLoad::linked(4);
        assert_eq!(load.batch(), 4);
        for _ in 0..3 {
            assert!(load.datagrams() > Duration::ZERO);
            assert!(load.keep_alives() > Duration::ZERO);
            assert!(load.frame() > Duration::ZERO);
        }
        assert!(ServerLoad::retried(HOLD_BATCH).handshakes() > Duration::ZERO);
        let mut flown = ServerLoad::flown(HOLD_BATCH);
        assert!(flown.handshake_ends() > Duration::ZERO);
        flown.frames();
        assert_eq!(flown.server.linked().count(), HOLD_BATCH);
    }
}
