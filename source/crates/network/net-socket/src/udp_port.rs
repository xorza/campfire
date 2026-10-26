#![expect(
    unsafe_code,
    reason = "`polling` registers a socket with its poller by an unsafe call (02-engine-core.md, Unsafe only where an outside API asks)"
)]

use std::io::{self, ErrorKind, IoSliceMut};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use campfire_common::ErrorReport;
use polling::{Event, Events, PollMode, Poller};
use quinn_udp::{RecvMeta, Transmit, UdpSocketState};
use tracing::warn;

/// The poller's key of the socket.
const SOCKET: usize = 0;
/// The poller's key of the wake's receiving end.
const WAKE: usize = 1;

/// An endpoint's UDP socket, read and written in batches through `quinn-udp`, and the poller its
/// thread waits on: for the socket to be readable, for a deadline, or for the app's wake
/// (11-net-stack/packet-session.md, One endpoint core and one pass for each IO). The socket is
/// non-blocking, as `quinn-udp` makes it (11-net-stack/udp.md, The socket stays on `quinn-udp`,
/// with GSO and GRO).
#[derive(Debug)]
pub(crate) struct UdpPort {
    socket: UdpSocket,
    state: UdpSocketState,
    poller: Poller,
    /// Level where the poller has it, so a wait sets nothing up again; else oneshot, which each
    /// wait arms anew.
    mode: PollMode,
    wake: Wake,
}

/// The app's wake of the thread: a byte it sends from one loopback datagram socket to the other,
/// which the poller watches, as the self-pipe trick does, and only while no byte is pending, so
/// one byte at most is ever in flight (11-net-stack/udp.md, A wake sends one byte until a wait
/// takes it). `polling`'s own `notify` allocates on each call where it runs on kqueue, as on
/// macOS, and a frame that sends wakes the thread.
#[derive(Debug)]
struct Wake {
    sender: UdpSocket,
    receiver: UdpSocket,
    pending: AtomicBool,
}

impl UdpPort {
    pub(crate) fn bind(address: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        let state = UdpSocketState::new((&socket).into())?;
        let poller = Poller::new()?;
        let mode = if poller.supports_level() {
            PollMode::Level
        } else {
            PollMode::Oneshot
        };
        let wake = Wake::pair()?;
        Self::watch(&poller, &socket, SOCKET, mode)?;
        Self::watch(&poller, &wake.receiver, WAKE, mode)?;
        Ok(Self {
            socket,
            state,
            poller,
            mode,
            wake,
        })
    }

    fn watch(poller: &Poller, socket: &UdpSocket, key: usize, mode: PollMode) -> io::Result<()> {
        // SAFETY: `UdpPort::drop` deletes each socket from this poller before the socket drops
        // (11-net-stack/api.md, No `unsafe` but what an outside API requires).
        unsafe { poller.add_with_mode(socket, Event::readable(key), mode) }
    }

    pub(crate) fn local_address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// The most datagrams of one transmit the socket sends at once, through GSO.
    pub(crate) fn max_gso_segments(&self) -> usize {
        self.state.max_gso_segments()
    }

    /// The most datagrams of one read into one buffer, through GRO.
    pub(crate) fn gro_segments(&self) -> usize {
        self.state.gro_segments()
    }

    /// Waits until the socket is readable, `timeout` passes, or `wake` is called; a wake that came
    /// since the last wait returns at once.
    pub(crate) fn wait(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        if matches!(self.mode, PollMode::Oneshot) {
            self.poller.modify(&self.socket, Event::readable(SOCKET))?;
            self.poller
                .modify(&self.wake.receiver, Event::readable(WAKE))?;
        }
        events.clear();
        self.poller.wait(events, timeout)?;
        // A wake that came is drained, and only then: in level mode an undrained wake would end
        // the next wait at once, and in oneshot mode the wait arms it anew.
        if events.iter().any(|event| event.key == WAKE) {
            self.wake.take();
        }
        Ok(())
    }

    /// Ends the thread's wait, or its next one. A wake allocates nothing, as each frame that
    /// sends calls it.
    pub(crate) fn wake(&self) {
        self.wake.send();
    }

    /// Reads one batch; `WouldBlock` when none waits. A peer's ICMP refusal, which Windows
    /// reports on a later read, is no datagram, and the read goes on.
    pub(crate) fn recv(
        &self,
        buffers: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> io::Result<usize> {
        loop {
            match self.state.recv((&self.socket).into(), buffers, meta) {
                Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
                read => return read,
            }
        }
    }

    /// Sends one transmit; every error is the caller's, as `quinn-udp` logs none.
    pub(crate) fn send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        self.state.try_send((&self.socket).into(), transmit)
    }
}

impl Wake {
    /// Two loopback sockets, each connected to the other, so no other sender reaches the
    /// receiver; both non-blocking.
    fn pair() -> io::Result<Self> {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        sender.connect(receiver.local_addr()?)?;
        receiver.connect(sender.local_addr()?)?;
        receiver.set_nonblocking(true)?;
        sender.set_nonblocking(true)?;
        Ok(Self {
            sender,
            receiver,
            pending: AtomicBool::new(false),
        })
    }

    /// Sends a byte, unless one is pending. A send that fails leaves none pending, so the next
    /// wake sends again.
    fn send(&self) {
        if self.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        match self.sender.send(&[0]) {
            Ok(_) => {}
            // A full buffer holds bytes the thread has yet to take.
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => {
                self.pending.store(false, Ordering::Release);
                warn!(error = %ErrorReport::of(&error), "the IO thread's wake failed");
            }
        }
    }

    /// Takes the pending byte, so the next wait waits until another wake. The byte is drained
    /// before the flag clears, and the clear releases the drain, so a wake sends its byte only
    /// after the last one left the socket; a wake between the two sends none, and the clear
    /// acquires that wake's swap, so the pass after this wait reads what it brought, as libuv's
    /// `uv__async_io` clears its `pending` by a read-modify-write after it reads its pipe
    /// (11-net-stack/udp.md, A wake sends one byte until a wait takes it).
    fn take(&self) {
        let mut byte = [0; 1];
        while self.receiver.recv(&mut byte).is_ok() {}
        let _ = self.pending.swap(false, Ordering::AcqRel);
    }
}

impl Drop for UdpPort {
    fn drop(&mut self) {
        for socket in [&self.socket, &self.wake.receiver] {
            if let Err(error) = self.poller.delete(socket) {
                warn!(error = %ErrorReport::of(&error), "a UDP socket did not leave its poller");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wake_ends_the_next_wait_once_and_a_wait_takes_every_wake_that_came() {
        let port = UdpPort::bind((Ipv4Addr::LOCALHOST, 0).into()).unwrap();
        let mut events = Events::new();
        let woke = |events: &Events| events.iter().any(|event| event.key == WAKE);
        // Two wakes before a wait: it ends on the wake, and takes both, as the second sent no
        // byte; a loopback datagram may reach its socket a moment after its send, as on macOS,
        // which a second byte in flight would end the next wait with.
        port.wake();
        port.wake();
        port.wait(&mut events, None).unwrap();
        assert!(woke(&events));
        // None since: a wait of no time finds nothing. A wake then ends a wait, which waits for
        // its byte.
        port.wait(&mut events, Some(Duration::ZERO)).unwrap();
        assert!(!woke(&events));
        port.wake();
        port.wait(&mut events, Some(Duration::from_secs(5)))
            .unwrap();
        assert!(woke(&events));
    }
}
