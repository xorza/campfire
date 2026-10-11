use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Instant;

use campfire_log::ErrorReport;
use polling::Events;
use tracing::{error, warn};

use crate::outbox::Outbox;
use crate::recv_batch::RecvBatch;
use crate::shared_endpoint::SharedEndpoint;

/// An endpoint's thread: it waits for the socket, a deadline or a wake, reads one batch with no
/// lock, gives it to the endpoint and serves the connections under the mutex, then writes what
/// they send with no lock (design 15, D3). It ends after the pass that writes an endpoint's
/// closes.
#[derive(Debug)]
pub(crate) struct IoThread {
    shared: Arc<SharedEndpoint>,
    events: Events,
    batch: RecvBatch,
    outbox: Outbox,
}

impl IoThread {
    pub(crate) fn new(shared: Arc<SharedEndpoint>, max_payload: usize) -> IoThread {
        let batch = RecvBatch::new(shared.port(), max_payload);
        IoThread {
            shared,
            events: Events::new(),
            batch,
            outbox: Outbox::default(),
        }
    }

    pub(crate) fn run(mut self) {
        let mut idle = true;
        loop {
            if idle {
                let deadline = self.shared.lock().next_deadline();
                let timeout =
                    deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
                if let Err(error) = self.shared.port().wait(&mut self.events, timeout) {
                    error!(error = %ErrorReport::of(&error), "the QUIC thread cannot wait on its socket; its links end by their idle timeout");
                    return;
                }
            }
            let filled = match self.batch.read(self.shared.port()) {
                Ok(filled) => filled,
                Err(error) if error.kind() == ErrorKind::WouldBlock => 0,
                // A peer's ICMP refusal, which Windows reports on the next read: harmless to UDP.
                Err(error) if error.kind() == ErrorKind::ConnectionReset => {
                    idle = false;
                    continue;
                }
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the QUIC socket's read failed");
                    0
                }
            };
            let now = Instant::now();
            let closing = {
                let mut state = self.shared.lock();
                for datagram in self.batch.datagrams(filled) {
                    state.take(now, datagram, &mut self.outbox);
                }
                state.serve(now, &mut self.outbox);
                state.closing()
            };
            self.outbox.send(self.shared.port());
            if closing {
                return;
            }
            idle = filled == 0;
        }
    }
}
