//! An endpoint as its host and its IO thread share it: its core behind one mutex, and its IO, which
//! the thread reads and waits on outside it (13-net-stack/session.md, One endpoint core and one
//! pass for each IO).

use std::sync::{Mutex, MutexGuard};

use campfire_net_emulator::LinkConditioner;
use campfire_net_payload::AppCode;

use crate::carrier::Carrier;
use crate::close_session::CloseSession;
use crate::datagram_io::Outbound;
use crate::datagram_io::{Datagram, DatagramIo};
use crate::endpoint_core::{EndpointCore, PeerKey};
use crate::endpoint_parts::EndpointParts;

/// An endpoint's core and IO, which a host locks for its own steps and an IO thread runs; the
/// carrier `C` names what carries each session for the host.
#[derive(Debug)]
pub struct EndpointRuntime<Io, C> {
    core: Mutex<EndpointCore<C>>,
    io: Io,
}

/// The most datagrams a pass opens under one hold of the endpoint's lock, for which the frame's
/// endpoint systems wait: quinn's `IO_LOOP_BOUND`, the lowest bound of its endpoint loop that did
/// not lower throughput in its benchmarks.
const PASS_DATAGRAMS: usize = 160;

/// The datagrams a pass reads and sends, which each pass reuses.
#[derive(Debug, Default)]
pub struct PassScratch {
    datagrams: Vec<Datagram>,
    outbound: Vec<Outbound>,
}

impl<Io: DatagramIo, C: Carrier> EndpointRuntime<Io, C> {
    pub fn new(parts: EndpointParts<Io>) -> Self {
        let EndpointParts { io, side, config } = parts;
        let conditioner = config.conditioner.map(LinkConditioner::new);
        Self {
            core: Mutex::new(EndpointCore::new(config.session, side, conditioner)),
            io,
        }
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
    /// until the endpoint closed all its sessions or its IO failed.
    pub fn run(&self) {
        let mut scratch = PassScratch::default();
        loop {
            let timeout = {
                let core = self.lock();
                if core.ended() {
                    break;
                }
                let now = core.now();
                core.next_deadline()
                    .map(|deadline| deadline.saturating_duration_since(now))
            };
            self.io.wait(timeout);
            self.pass(&mut scratch);
        }
    }
}

impl<Io: DatagramIo, C: Carrier> CloseSession for EndpointRuntime<Io, C> {
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
    use crate::endpoint_config::EndpointConfig;
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

    /// One pass over a burst of 600 datagrams reads them in batches of 160, each opened under
    /// its own hold of the lock, until a read finds fewer: 160, 160, 160 and 120.
    #[test]
    fn a_pass_reads_a_burst_in_bounded_batches() {
        let io = Burst {
            waiting: Mutex::new(600),
            reads: Mutex::new(Vec::new()),
        };
        let runtime = EndpointRuntime::<Burst, u8>::new(EndpointParts::server(
            io,
            Box::new(PlainEstablishment::default()),
            EndpointConfig::default(),
        ));
        runtime.pass(&mut PassScratch::default());
        assert_eq!(*runtime.io().reads.lock().unwrap(), [160, 160, 160, 120]);
    }
}
