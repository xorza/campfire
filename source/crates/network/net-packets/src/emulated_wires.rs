use bevy_platform::collections::HashMap;
use core::iter;
use core::net::SocketAddr;
use core::task::Waker;
use core::time::Duration;
use std::sync::{Arc, Mutex, MutexGuard};

use campfire_net_emulator::EmulatedIo;
use campfire_net_payload::DEFAULT_MTU;

use crate::datagram_arena::datagram_arena;
use crate::datagram_io::Outbound;
use crate::datagram_io::{Datagram, DatagramIo};
use crate::io_failed::IoFailed;
use crate::signal::Signal;
use campfire_net_bytes::ByteArena;

/// The emulated network's wires an endpoint holds: a client's one, a server's one for each client
/// (11-net-stack/layering.md, The emulated wires and `net-udp`'s emulated server and client are
/// production).
#[derive(Debug)]
pub struct EmulatedWires {
    wires: Mutex<Wires>,
    /// What each datagram the wires deliver is copied into, as a socket reads into its own
    /// buffers: the wire's bytes share their allocation with the sender's.
    arenas: Mutex<ByteArena>,
    signal: Arc<Signal>,
    local: SocketAddr,
}

/// The wires, and the index of each by its peer's address, which a send and an MTU find it by.
#[derive(Debug, Default)]
struct Wires {
    ends: Vec<EmulatedIo>,
    by_peer: HashMap<SocketAddr, usize>,
}

impl EmulatedWires {
    /// The wires of a server, which takes each client's by [`add`](Self::add).
    pub fn server() -> Self {
        Self {
            wires: Mutex::default(),
            arenas: Mutex::new(datagram_arena(DEFAULT_MTU)),
            signal: Arc::default(),
            local: EmulatedIo::SERVER_ADDR,
        }
    }

    /// The wires of a client, its one `wire`.
    pub fn client(wire: EmulatedIo) -> Self {
        let wires = Self {
            local: wire.client_address(),
            ..Self::server()
        };
        wires.add(wire);
        wires
    }

    /// Takes `wire`, whose packets to this end wake the IO thread.
    pub fn add(&self, wire: EmulatedIo) {
        wire.set_waker(Waker::from(Arc::clone(&self.signal)));
        let mut wires = self.lock();
        let index = wires.ends.len();
        wires.by_peer.insert(wire.remote_address(), index);
        wires.ends.push(wire);
    }

    fn lock(&self) -> MutexGuard<'_, Wires> {
        self.wires
            .lock()
            .expect("no thread panics holding the wires")
    }
}

impl DatagramIo for EmulatedWires {
    /// Reads every wire under one hold of the network.
    fn receive(&self, out: &mut Vec<Datagram>, max: usize) -> Result<(), IoFailed> {
        let mut arenas = self
            .arenas
            .lock()
            .expect("no thread panics holding the arenas");
        let wires = self.lock();
        if let Some(first) = wires.ends.first() {
            first
                .network()
                .receive_ends(&wires.ends, max, |end, bytes| {
                    let payload = arenas.copy(&bytes);
                    out.push(Datagram {
                        local: None,
                        from: end.remote_address(),
                        payload,
                    });
                });
        }
        Ok(())
    }

    /// Puts each run of datagrams to one peer on its wire under one hold of the network.
    fn send(&self, outbound: &mut Vec<Outbound>) {
        let wires = self.lock();
        let mut outbound = outbound.drain(..).peekable();
        while let Some(Outbound { to, datagram, .. }) = outbound.next() {
            let run = iter::once(datagram).chain(iter::from_fn(|| {
                outbound
                    .next_if(|next| next.to == to)
                    .map(|next| next.datagram)
            }));
            match wires.by_peer.get(&to) {
                Some(&index) => wires.ends[index].send(run),
                None => run.for_each(drop),
            }
        }
    }

    fn local_address(&self) -> SocketAddr {
        self.local
    }

    fn mtu(&self, peer: SocketAddr) -> usize {
        let wires = self.lock();
        wires
            .by_peer
            .get(&peer)
            .and_then(|&index| wires.ends[index].mtu())
            .unwrap_or(DEFAULT_MTU)
    }

    fn wait(&self, timeout: Option<Duration>) {
        let due = {
            let wires = self.lock();
            wires
                .ends
                .first()
                .and_then(|first| first.network().until_due_ends(&wires.ends))
        };
        let timeout = match (timeout, due) {
            (Some(timeout), Some(due)) => Some(timeout.min(due)),
            (timeout, due) => timeout.or(due),
        };
        self.signal.wait(timeout);
    }

    fn wake(&self) {
        self.signal.raise();
    }
}
