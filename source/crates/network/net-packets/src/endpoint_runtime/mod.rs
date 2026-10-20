//! An endpoint as its host and its IO thread share it: its core behind one mutex, and its IO, which
//! the thread reads and waits on outside it (11-net-stack/packet-session.md, One endpoint core and
//! one pass for each IO).

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use campfire_net_emulator::LinkConditioner;
use campfire_net_payload::AppCode;

use crate::carrier::Carrier;
use crate::close_packet_session::ClosePacketSession;
use crate::datagram_io::Outbound;
use crate::datagram_io::{Datagram, DatagramIo};
use crate::endpoint_core::{EndpointCore, HOLD_BATCH, HoldRest, PeerKey};
use crate::endpoint_pieces::EndpointPieces;
use crate::signal::Signal;

/// An endpoint's core and IO, which a host locks for its own steps and an IO thread runs; the
/// carrier `C` names what carries each packet session for the host.
#[derive(Debug)]
pub struct EndpointRuntime<Io, C> {
    core: Mutex<EndpointCore<C>>,
    io: Io,
    /// What the IO thread raises as a pass leaves the host work, so a host that waits for it runs
    /// its frame then (11-net-stack/packet-session.md, The IO thread wakes the frame).
    host_wake: OnceLock<Arc<Signal>>,
}

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
    /// timers at its own time, then what goes out; again while a read fills its batch or the core
    /// left work for its next hold, the next read leaving room for the datagrams it kept.
    pub fn pass(&self, scratch: &mut PassScratch) {
        self.pass_holding(scratch, &mut ());
    }

    /// A pass whose each hold of the lock `timing` runs.
    pub(crate) fn pass_holding(&self, scratch: &mut PassScratch, timing: &mut impl HoldTiming) {
        let mut rest = HoldRest::default();
        loop {
            let room = HOLD_BATCH.saturating_sub(rest.datagrams);
            let received = self.io.receive(&mut scratch.datagrams, room);
            let full = scratch.datagrams.len() >= room;
            if received.is_err() {
                timing.hold(|| self.lock().fail());
                return;
            }
            rest = timing.hold(|| {
                self.lock()
                    .pass(&mut scratch.datagrams, &mut scratch.outbound)
            });
            self.send(&mut scratch.outbound);
            if !full && rest.is_empty() {
                return;
            }
        }
    }

    /// Sends the datagrams the frame's work gave, `outbound`, and wakes the IO thread only when
    /// that work needs it awake.
    pub fn send_from_frame(&self, outbound: &mut Vec<Outbound>) {
        self.send(outbound);
        if self.lock().is_wake_due() {
            self.io.wake();
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
                let mut core = self.lock();
                if core.is_ended() {
                    break;
                }
                let now = core.now();
                core.arm()
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

/// What runs each hold of an endpoint's lock: a host's own, `()`, runs it as it comes, and a
/// bench's times it.
pub(crate) trait HoldTiming {
    fn hold<R>(&mut self, held: impl FnOnce() -> R) -> R;
}

impl HoldTiming for () {
    fn hold<R>(&mut self, held: impl FnOnce() -> R) -> R {
        held()
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::endpoint_core;

    /// The most items of work a hold of the endpoint's lock takes, as a bench fills the batch.
    pub const HOLD_BATCH: usize = endpoint_core::HOLD_BATCH;
}

#[cfg(test)]
mod tests;
