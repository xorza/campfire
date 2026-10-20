use std::io::{self, ErrorKind};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use campfire_common::ErrorReport;
use polling::{Event, Events};
use tracing::{error, info, warn};

use crate::port_mapper::{PortMapper, Routes};
use crate::thread_stop::ThreadStop;
use campfire_store::SharedPort;

/// The poller's key of the announcements' socket.
const ANNOUNCEMENTS: usize = 0;
/// The most bytes of an announcement the thread reads: a datagram of a network's MTU.
const DATAGRAM: usize = 1500;

/// What the thread waits on between its turns: the stop, and routers' announcements on their
/// socket, when it opened.
#[derive(Debug)]
pub(crate) struct Listening {
    stop: Arc<ThreadStop>,
    announcements: Option<SharedPort>,
    events: Events,
    datagram: Box<[u8; DATAGRAM]>,
}

impl Listening {
    /// The wait of `stop`, and the announcements' socket of `routes`, non-blocking and watched;
    /// a socket that does not open is logged, and the map hears no announcement.
    pub(crate) fn open(stop: Arc<ThreadStop>, routes: Routes) -> io::Result<Self> {
        let announcements = match routes.announcements() {
            Ok(port) => {
                port.socket().set_nonblocking(true)?;
                // SAFETY: `Listening::drop` deletes the socket from this poller before it drops
                // (11-net-stack/api.md, No `unsafe` but what an outside API requires).
                unsafe {
                    stop.poller
                        .add(port.socket(), Event::readable(ANNOUNCEMENTS))?;
                }
                Some(port)
            }
            Err(error) => {
                info!(error = %ErrorReport::of(&error), "the server hears no router's announcement; it finds a router by its searches alone");
                None
            }
        };
        Ok(Self {
            stop,
            announcements,
            events: Events::new(),
            datagram: Box::new([0; DATAGRAM]),
        })
    }

    /// The thread: a turn of `mapper`, then a wait until its next, which a stop ends, and an
    /// announcement that starts a turn cuts short; at the stop, the mapping deleted.
    pub(crate) fn run(mut self, mut mapper: PortMapper) {
        while self.turn(&mut mapper) {}
        mapper.end();
    }

    /// A turn of `mapper` and the wait after it: whether another turn comes, which none does once
    /// stopped, or once the wait fails.
    fn turn(&mut self, mapper: &mut PortMapper) -> bool {
        let next = Instant::now() + mapper.turn(&self.stop.stopped);
        loop {
            if self.stop.stopped.load(Ordering::SeqCst) {
                return false;
            }
            let timeout = next.saturating_duration_since(Instant::now());
            if timeout.is_zero() {
                return true;
            }
            if let Some(port) = &self.announcements
                && let Err(error) = self
                    .stop
                    .poller
                    .modify(port.socket(), Event::readable(ANNOUNCEMENTS))
            {
                warn!(error = %ErrorReport::of(&error), "the port map's thread cannot watch for announcements");
            }
            self.events.clear();
            if let Err(error) = self.stop.poller.wait(&mut self.events, Some(timeout)) {
                error!(error = %ErrorReport::of(&error), "the port map's thread cannot wait; it deletes its mapping and ends");
                return false;
            }
            if self.hear(mapper) {
                return true;
            }
        }
    }

    /// Reads each announcement that waits: whether one starts a turn at once.
    fn hear(&mut self, mapper: &mut PortMapper) -> bool {
        let Some(port) = &self.announcements else {
            return false;
        };
        let mut starts = false;
        loop {
            match port.socket().recv_from(self.datagram.as_mut_slice()) {
                Ok((read, from)) => starts |= mapper.hear(&self.datagram[..read], from),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return starts,
                Err(error) => {
                    warn!(error = %ErrorReport::of(&error), "the announcements' socket failed");
                    return starts;
                }
            }
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        if let Some(port) = &self.announcements {
            self.stop.poller.delete(port.socket()).ok();
        }
    }
}
