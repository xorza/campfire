//! An endpoint as its host and its IO thread share it: its core behind one mutex, and its IO, which
//! the thread reads and waits on outside it (13-net-stack/packet-session.md, One endpoint core and
//! one pass for each IO).

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use campfire_net_emulator::LinkConditioner;
use campfire_net_payload::AppCode;

use crate::carrier::Carrier;
use crate::close_packet_session::ClosePacketSession;
use crate::datagram_io::Outbound;
use crate::datagram_io::{Datagram, DatagramIo};
use crate::endpoint_core::{EndpointCore, PeerKey};
use crate::endpoint_pieces::EndpointPieces;
use crate::signal::Signal;

/// An endpoint's core and IO, which a host locks for its own steps and an IO thread runs; the
/// carrier `C` names what carries each packet session for the host.
#[derive(Debug)]
pub struct EndpointRuntime<Io, C> {
    core: Mutex<EndpointCore<C>>,
    io: Io,
    /// What the IO thread raises as a pass leaves the host work, so a host that waits for it runs
    /// its frame then (13-net-stack/packet-session.md, The IO thread wakes the frame).
    host_wake: OnceLock<Arc<Signal>>,
}

/// The most datagrams a pass opens under one hold of the endpoint's lock, for which the frame's
/// endpoint systems wait: quinn's `IO_LOOP_BOUND`, the lowest bound of its endpoint loop that did
/// not lower throughput in its benchmarks (13-net-stack/packet-session.md, A pass opens a bounded
/// batch under each hold of the lock).
const PASS_DATAGRAMS: usize = 160;

/// The datagrams a pass reads and sends, which each pass reuses.
#[derive(Debug, Default)]
pub struct PassScratch {
    datagrams: Vec<Datagram>,
    outbound: Vec<Outbound>,
}

impl<Io: DatagramIo, C: Carrier> EndpointRuntime<Io, C> {
    pub fn new(parts: EndpointPieces<Io>) -> Self {
        let EndpointPieces { io, side, setup } = parts;
        let conditioner = setup.conditioner.map(LinkConditioner::new);
        Self {
            core: Mutex::new(EndpointCore::new(setup.packet_session, side, conditioner)),
            io,
            host_wake: OnceLock::new(),
        }
    }

    /// Wakes `wake` from the IO thread after each pass that leaves the host work; the first wake
    /// a runtime is given stays.
    pub fn wake_host_with(&self, wake: Arc<Signal>) {
        self.host_wake.get_or_init(|| wake);
    }

    pub const fn io(&self) -> &Io {
        &self.io
    }

    pub fn lock(&self) -> MutexGuard<'_, EndpointCore<C>> {
        self.core
            .lock()
            .expect("no thread panics holding an endpoint")
    }

    /// One pass: a batch of the IO's datagrams, read outside the lock, the core over them and its
    /// timers at its own time, then what goes out; again while a read fills its batch.
    pub fn pass(&self, scratch: &mut PassScratch) {
        loop {
            let received = self.io.receive(&mut scratch.datagrams, PASS_DATAGRAMS);
            let full = scratch.datagrams.len() >= PASS_DATAGRAMS;
            let mut core = self.lock();
            if received.is_err() {
                core.fail();
                return;
            }
            core.pass(&mut scratch.datagrams, &mut scratch.outbound);
            drop(core);
            self.send(&mut scratch.outbound);
            if !full {
                return;
            }
        }
    }

    /// Sends each datagram of `outbound`, and empties it.
    pub fn send(&self, outbound: &mut Vec<Outbound>) {
        if !outbound.is_empty() {
            self.io.send(outbound);
        }
    }

    /// The IO thread: it waits for a datagram, its next deadline or a wake, and runs a pass,
    /// until the endpoint closed all its packet sessions or its IO failed.
    pub fn run(&self) {
        let mut scratch = PassScratch::default();
        loop {
            let timeout = {
                let core = self.lock();
                if core.is_ended() {
                    break;
                }
                let now = core.now();
                core.next_deadline()
                    .map(|deadline| deadline.saturating_duration_since(now))
            };
            self.io.wait(timeout);
            self.pass(&mut scratch);
            self.wake_host();
        }
    }

    /// Raises the host's wake when the passes so far left it work.
    fn wake_host(&self) {
        if self.lock().take_host_due()
            && let Some(wake) = self.host_wake.get()
        {
            wake.raise();
        }
    }
}

impl<Io: DatagramIo, C: Carrier> ClosePacketSession for EndpointRuntime<Io, C> {
    fn close(&self, key: PeerKey, code: AppCode) {
        let mut outbound = Vec::new();
        {
            self.lock().close(key, code, &mut outbound);
        }
        self.send(&mut outbound);
        self.io.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint_setup::EndpointSetup;
    use crate::io_failed::IoFailed;
    use crate::plain_establishment::PlainEstablishment;
    use bytes::BytesMut;
    use core::net::{Ipv4Addr, SocketAddr};
    use core::time::Duration;

    /// An IO with `waiting` datagrams of noise, which records what each read appended.
    #[derive(Debug)]
    struct Burst {
        waiting: Mutex<usize>,
        reads: Mutex<Vec<usize>>,
    }

    impl DatagramIo for Burst {
        fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
            let mut waiting = self.waiting.lock().unwrap();
            let read = max.min(*waiting);
            *waiting -= read;
            self.reads.lock().unwrap().push(read);
            out.extend((0..read).map(|_| Datagram {
                from: SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
                payload: BytesMut::from(&[0_u8; 4][..]),
            }));
            Ok(())
        }

        fn send(&self, outbound: &mut Vec<Outbound>) {
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
            Burst {
                waiting: Mutex::new(1),
                reads: Mutex::new(Vec::new()),
            },
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

    /// One pass over a burst of 600 datagrams reads them in batches of 160, each opened under
    /// its own hold of the lock, until a read finds fewer: 160, 160, 160 and 120.
    #[test]
    fn a_pass_reads_a_burst_in_bounded_batches() {
        let io = Burst {
            waiting: Mutex::new(600),
            reads: Mutex::new(Vec::new()),
        };
        let runtime = EndpointRuntime::<Burst, u8>::new(EndpointPieces::server(
            io,
            Box::new(PlainEstablishment::default()),
            EndpointSetup::default(),
        ));
        runtime.pass(&mut PassScratch::default());
        assert_eq!(*runtime.io().reads.lock().unwrap(), [160, 160, 160, 120]);
    }
}
