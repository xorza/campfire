use std::io::{self, ErrorKind, IoSliceMut};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use campfire_log::ErrorReport;
use polling::{Event, Events, Poller};
use quinn_udp::{RecvMeta, Transmit, UdpSocketState};
use tracing::warn;

/// The poller's key of the socket.
const SOCKET: usize = 0;

/// An endpoint's UDP socket, read and written in batches through `quinn-udp`, and the poller its
/// thread waits on: for the socket to be readable, for a deadline, or for the app's wake (design
/// 15, D3). The socket is non-blocking, as `quinn-udp` makes it.
#[derive(Debug)]
pub(crate) struct UdpPort {
    socket: UdpSocket,
    state: UdpSocketState,
    poller: Poller,
}

impl UdpPort {
    pub(crate) fn bind(address: SocketAddr) -> io::Result<UdpPort> {
        let socket = UdpSocket::bind(address)?;
        let state = UdpSocketState::new((&socket).into())?;
        let poller = Poller::new()?;
        UdpPort::watch(&poller, &socket)?;
        Ok(UdpPort {
            socket,
            state,
            poller,
        })
    }

    #[expect(
        unsafe_code,
        reason = "polling's add, whose contract the port keeps: it deletes the socket from the poller in its drop, before either closes"
    )]
    fn watch(poller: &Poller, socket: &UdpSocket) -> io::Result<()> {
        // SAFETY: `UdpPort::drop` deletes the socket from this poller before the socket drops.
        unsafe { poller.add(socket, Event::readable(SOCKET)) }
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
        events.clear();
        self.poller.wait(events, timeout)?;
        Ok(())
    }

    pub(crate) fn wake(&self) {
        if let Err(error) = self.poller.notify() {
            warn!(error = %ErrorReport::of(&error), "the QUIC thread's wake failed");
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

impl Drop for UdpPort {
    fn drop(&mut self) {
        if let Err(error) = self.poller.delete(&self.socket) {
            warn!(error = %ErrorReport::of(&error), "the QUIC socket did not leave its poller");
        }
    }
}
