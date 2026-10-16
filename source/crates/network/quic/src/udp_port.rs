use std::io::{self, ErrorKind, IoSliceMut};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use campfire_log::ErrorReport;
use polling::{Event, Events, Poller};
use quinn_udp::{RecvMeta, Transmit, UdpSocketState};
use tracing::warn;

/// The poller's key of the socket.
const SOCKET: usize = 0;
/// The poller's key of the wake's receiving end.
const WAKE: usize = 1;

/// An endpoint's UDP socket, read and written in batches through `quinn-udp`, and the poller its
/// thread waits on: for the socket to be readable, for a deadline, or for the app's wake (design
/// 15, D3). The socket is non-blocking, as `quinn-udp` makes it.
#[derive(Debug)]
pub(crate) struct UdpPort {
    socket: UdpSocket,
    state: UdpSocketState,
    poller: Poller,
    wake: Wake,
}

/// The app's wake of the thread: a byte it sends from one loopback datagram socket to the other,
/// which the poller watches, as the self-pipe trick does. `polling`'s own `notify` allocates on
/// each call where it runs on kqueue, as on macOS, and a frame that sends wakes the thread.
#[derive(Debug)]
struct Wake {
    sender: UdpSocket,
    receiver: UdpSocket,
}

impl UdpPort {
    pub(crate) fn bind(address: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        let state = UdpSocketState::new((&socket).into())?;
        let poller = Poller::new()?;
        let wake = Wake::pair()?;
        Self::watch(&poller, &socket, SOCKET)?;
        Self::watch(&poller, &wake.receiver, WAKE)?;
        Ok(Self {
            socket,
            state,
            poller,
            wake,
        })
    }

    fn watch(poller: &Poller, socket: &UdpSocket, key: usize) -> io::Result<()> {
        // SAFETY: `UdpPort::drop` deletes each socket from this poller before the socket drops.
        unsafe { poller.add(socket, Event::readable(key)) }
    }

    pub(crate) fn local_address(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Whether the OS may fragment a datagram the socket sends, which path MTU discovery forbids.
    pub(crate) fn may_fragment(&self) -> bool {
        self.state.may_fragment()
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
        self.poller.modify(&self.socket, Event::readable(SOCKET))?;
        self.poller
            .modify(&self.wake.receiver, Event::readable(WAKE))?;
        events.clear();
        self.poller.wait(events, timeout)?;
        self.wake.drain();
        Ok(())
    }

    /// Ends the thread's wait, or its next one. A wake allocates nothing, as each frame that
    /// sends calls it.
    pub(crate) fn wake(&self) {
        match self.wake.sender.send(&[0]) {
            // A full buffer holds wakes the thread has yet to take.
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => {
                warn!(error = %ErrorReport::of(&error), "the QUIC thread's wake failed");
            }
        }
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

    pub(crate) fn send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        self.state.send((&self.socket).into(), transmit)
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
        Ok(Self { sender, receiver })
    }

    /// Takes every wake that came, so the next wait waits until another.
    fn drain(&self) {
        let mut byte = [0; 1];
        while self.receiver.recv(&mut byte).is_ok() {}
    }
}

impl Drop for UdpPort {
    fn drop(&mut self) {
        for socket in [&self.socket, &self.wake.receiver] {
            if let Err(error) = self.poller.delete(socket) {
                warn!(error = %ErrorReport::of(&error), "a QUIC socket did not leave its poller");
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
        // Two wakes before a wait: it ends at once on the wake, and takes both.
        port.wake();
        port.wake();
        port.wait(&mut events, None).unwrap();
        assert!(woke(&events));
        // None since: a wait of no time finds nothing. A wake then ends a wait, which waits for
        // it: a loopback datagram may reach its socket a moment after its send, as on macOS.
        port.wait(&mut events, Some(Duration::ZERO)).unwrap();
        assert!(!woke(&events));
        port.wake();
        port.wait(&mut events, Some(Duration::from_secs(5)))
            .unwrap();
        assert!(woke(&events));
    }
}
