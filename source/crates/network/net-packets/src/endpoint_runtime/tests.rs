use super::*;
use crate::connection_id::ConnectionId;
use crate::endpoint_core::{EndpointTime, News};
use crate::endpoint_setup::EndpointSetup;
use crate::establish::{Establish, Establishing, PathCheck, ReadCost, ServerEstablish};
use crate::fixtures::hold_clock::HoldClock;
use crate::header::IO_BIT;
use crate::io_failed::IoFailed;
use crate::plain_establishment::PlainEstablishment;
use bytes::BytesMut;
use campfire_net_core::Instant;
use campfire_net_payload::{OutgoingPayload, PacketSessionQueues, PayloadKind, PayloadToken};
use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

/// An IO with `waiting` datagrams, each of 4 bytes from `first`, and each from an address of its
/// own, which records what each read appended and counts the datagrams sent.
#[derive(Debug)]
struct Burst {
    waiting: Mutex<usize>,
    first: u8,
    reads: Mutex<Vec<usize>>,
    sent: AtomicUsize,
}

impl Burst {
    fn new(waiting: usize, first: u8) -> Self {
        Self {
            waiting: Mutex::new(waiting),
            first,
            reads: Mutex::new(Vec::new()),
            sent: AtomicUsize::new(0),
        }
    }
}

impl DatagramIo for Burst {
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
        let mut waiting = self.waiting.lock().unwrap();
        let read = max.min(*waiting);
        *waiting -= read;
        let mut reads = self.reads.lock().unwrap();
        let before: usize = reads.iter().sum();
        reads.push(read);
        out.extend((before..before + read).map(|nth| Datagram {
            from: SocketAddr::from((Ipv4Addr::LOCALHOST, 1000 + u16::try_from(nth).unwrap())),
            local: None,
            payload: BytesMut::from(&[self.first, 0, 0, 0][..]),
        }));
        Ok(())
    }

    fn send(&self, outbound: &mut Vec<Outbound>) {
        self.sent.fetch_add(outbound.len(), Ordering::Relaxed);
        outbound.clear();
    }

    fn local_address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 2))
    }

    fn mtu(&self, _peer: SocketAddr) -> usize {
        1200
    }

    fn wait(&self, _timeout: Option<Duration>) {}

    fn wake(&self) {}
}

/// A pass that opens a packet session leaves the host its news, which wakes the host until it
/// takes them; a pass that leaves it nothing new then wakes nothing.
#[test]
fn a_pass_wakes_the_host_while_it_has_work() {
    let runtime = EndpointRuntime::<Burst, u8>::new(EndpointPieces::server(
        Burst::new(1, 0),
        Box::new(PlainEstablishment::default()),
        EndpointSetup::default(),
    ));
    let wake = Arc::new(Signal::default());
    runtime.wake_host_with(Arc::clone(&wake));
    runtime.lock().start(0, &mut Vec::new());
    let mut scratch = PassScratch::default();
    runtime.pass(&mut scratch);
    runtime.wake_host();
    assert!(wake.take_raised());
    runtime.wake_host();
    assert!(
        wake.take_raised(),
        "news the host has not taken wakes it again"
    );
    while runtime.lock().next_news().is_some() {}
    runtime.pass(&mut scratch);
    runtime.wake_host();
    assert!(!wake.take_raised());
}

/// An establishment that starts a handshake from each datagram it reads, and counts them.
#[derive(Debug, Default)]
struct Starts {
    started: Arc<AtomicUsize>,
}

impl Establish for Starts {
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) -> ReadCost {
        self.started.fetch_add(1, Ordering::Relaxed);
        ReadCost::Handshake
    }

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ServerEstablish for Starts {
    fn unknown(&mut self, _: Instant, _: &Datagram, _: &mut Establishing) {}

    fn set_accepting(&mut self, _: bool) {}

    fn path_check(&mut self) -> Option<&mut dyn PathCheck> {
        None
    }
}

/// One pass over a burst of 600 datagrams routes 120 of each hold's batch of 160, as a hold keeps
/// 40 for the packet sessions due, and each read fills the room the hold left: 160, then 120, 120
/// and 120, and the last 80. Each from an address of its own opens a packet session, which sends
/// its first `Ping`; a second after, all 600 are due, and a pass with no datagram polls them in
/// holds of a batch each, 160, 160, 160 and 120, each sending a `Ping` again. Over a burst of 300
/// handshakes, each hold starts one and keeps the rest of its batch, and each read fills only
/// the room the batch has: 160, then one for each hold until the IO is empty, 140 of them, then
/// nothing while the holds take the 159 kept.
#[test]
fn a_pass_reads_a_burst_in_bounded_batches() {
    let runtime = EndpointRuntime::<Burst, u8>::new(EndpointPieces::server(
        Burst::new(600, 0),
        Box::new(PlainEstablishment::default()),
        EndpointSetup::default(),
    ));
    runtime.lock().start(0, &mut Vec::new());
    let mut clock = HoldClock::default();
    let mut scratch = PassScratch::default();
    runtime.pass_holding(&mut scratch, &mut clock);
    assert_eq!(
        *runtime.io().reads.lock().unwrap(),
        [160, 120, 120, 120, 80]
    );
    assert_eq!(clock.iter().count(), 5);
    assert_eq!(runtime.io().sent.load(Ordering::Relaxed), 600);
    let keep_alive = EndpointSetup::default().packet_session.keep_alive;
    runtime
        .lock()
        .set_time(EndpointTime::Driven(Instant::ZERO + keep_alive));
    clock.clear();
    runtime.pass_holding(&mut scratch, &mut clock);
    assert_eq!(clock.iter().count(), 4);
    assert_eq!(runtime.io().sent.load(Ordering::Relaxed), 1200);

    let starts = Starts::default();
    let started = Arc::clone(&starts.started);
    let runtime = EndpointRuntime::<Burst, u8>::new(EndpointPieces::server(
        Burst::new(300, IO_BIT),
        Box::new(starts),
        EndpointSetup::default(),
    ));
    let mut clock = HoldClock::default();
    runtime.pass_holding(&mut PassScratch::default(), &mut clock);
    let mut reads = vec![160];
    reads.extend([1; 140]);
    reads.extend([0; 159]);
    assert_eq!(*runtime.io().reads.lock().unwrap(), reads);
    assert_eq!(clock.iter().count(), 300);
    assert_eq!(started.load(Ordering::Relaxed), 300);
}

/// One end of a lossless loopback of two, which lands each datagram it sends in its peer's inbox
/// at once, and counts the wakes its IO thread is given.
#[derive(Debug)]
struct Loopback {
    end: usize,
    inboxes: Arc<Mutex<[Vec<Datagram>; 2]>>,
    wakes: AtomicUsize,
}

impl Loopback {
    const ADDRESSES: [SocketAddr; 2] = [
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1)),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 2)),
    ];

    fn pair() -> [Self; 2] {
        let inboxes = Arc::default();
        [0, 1].map(|end| Self {
            end,
            inboxes: Arc::clone(&inboxes),
            wakes: AtomicUsize::new(0),
        })
    }

    fn is_waiting(&self) -> bool {
        !self.inboxes.lock().unwrap()[self.end].is_empty()
    }

    fn wakes(&self) -> usize {
        self.wakes.load(Ordering::Relaxed)
    }
}

impl DatagramIo for Loopback {
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
        let mut inboxes = self.inboxes.lock().unwrap();
        let inbox = &mut inboxes[self.end];
        out.extend(inbox.drain(..max.min(inbox.len())));
        Ok(())
    }

    fn send(&self, outbound: &mut Vec<Outbound>) {
        let from = self.local_address();
        self.inboxes.lock().unwrap()[1 - self.end].extend(outbound.drain(..).map(|outbound| {
            Datagram {
                from,
                local: None,
                payload: BytesMut::from(&*outbound.datagram),
            }
        }));
    }

    fn local_address(&self) -> SocketAddr {
        Self::ADDRESSES[self.end]
    }

    fn mtu(&self, _peer: SocketAddr) -> usize {
        1200
    }

    fn wait(&self, _timeout: Option<Duration>) {}

    fn wake(&self) {
        self.wakes.fetch_add(1, Ordering::Relaxed);
    }
}

/// An endpoint on a driven clock whose IO thread's steps the test runs as `run` would: a pass
/// once a datagram waits, its deadline comes or a frame woke it, then the deadline it sleeps to.
#[derive(Debug)]
struct Stepped {
    runtime: EndpointRuntime<Loopback, u8>,
    key: Option<PeerKey>,
    queues: PacketSessionQueues,
    sleeps_to: Option<Instant>,
    wakes_seen: usize,
    scratch: PassScratch,
    outbound: Vec<Outbound>,
}

impl Stepped {
    fn new(parts: EndpointPieces<Loopback>) -> Self {
        Self {
            runtime: EndpointRuntime::new(parts),
            key: None,
            queues: PacketSessionQueues::default(),
            sleeps_to: None,
            wakes_seen: 0,
            scratch: PassScratch::default(),
            outbound: Vec::new(),
        }
    }

    fn start(&mut self) {
        self.runtime.lock().start(0, &mut self.outbound);
        self.runtime.send(&mut self.outbound);
        self.pass();
    }

    fn pass(&mut self) {
        self.runtime.pass(&mut self.scratch);
        self.sleeps_to = self.runtime.lock().arm();
        self.wakes_seen = self.runtime.io().wakes();
    }

    /// The IO thread's step at `now`, then the frame's: the host carries the packet session that
    /// linked, and hands over and takes over its queues.
    fn frame(&mut self, now: Instant) {
        self.runtime.lock().set_time(EndpointTime::Driven(now));
        let io = self.runtime.io();
        if io.wakes() > self.wakes_seen
            || io.is_waiting()
            || self.sleeps_to.is_some_and(|deadline| deadline <= now)
        {
            self.pass();
        }
        let mut core = self.runtime.lock();
        while let Some(news) = core.next_news() {
            if let News::Linked { key, .. } = news {
                core.carry(key, 0, 1200);
                self.key = Some(key);
            }
        }
        if let Some(key) = self.key {
            core.hand_over(key, &mut self.queues);
            core.take_over(key, &mut self.queues, &mut self.outbound);
        }
        drop(core);
        self.runtime.send_from_frame(&mut self.outbound);
    }
}

/// A linked client at 60 frames a second, whose packet session sends a keep-alive each second:
/// its frames with nothing to send wake its IO thread 0 times in 100, past a keep-alive and
/// its ACK, and a payload, whose probe comes before the keep-alive the thread sleeps to, wakes it
/// once; the frames after it, which leave the deadline the thread then sleeps to, none.
#[test]
fn a_frame_wakes_the_io_thread_only_for_a_deadline_it_does_not_know() {
    let [client_io, server_io] = Loopback::pair();
    let mut client = Stepped::new(EndpointPieces::client(
        client_io,
        Loopback::ADDRESSES[1],
        Box::new(PlainEstablishment::default()),
        EndpointSetup::default(),
    ));
    let mut server = Stepped::new(EndpointPieces::server(
        server_io,
        Box::new(PlainEstablishment::default()),
        EndpointSetup::default(),
    ));
    server.start();
    client.start();
    let at = |frame: u64| Instant::from_elapsed(Duration::from_micros(frame * 16_667));
    let mut frame = 0;
    let mut frames = |client: &mut Stepped, server: &mut Stepped, count: u64| {
        for _ in 0..count {
            frame += 1;
            server.frame(at(frame));
            client.frame(at(frame));
        }
    };
    frames(&mut client, &mut server, 100);
    assert!(client.key.is_some() && server.key.is_some());
    assert_eq!(client.runtime.io().wakes(), 0);

    let payload = OutgoingPayload::buffer(b"data");
    client
        .queues
        .send(PayloadToken(1), PayloadKind::new(1), payload);
    let keep_alive = client
        .sleeps_to
        .expect("a linked packet session keeps alive");
    frames(&mut client, &mut server, 1);
    assert!(client.runtime.lock().next_deadline() < Some(keep_alive));
    assert_eq!(client.runtime.io().wakes(), 1);
    frames(&mut client, &mut server, 20);
    assert_eq!(client.runtime.io().wakes(), 1);
}
