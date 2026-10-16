use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
use std::ops::Range;

use quinn_proto::{EcnCodepoint, Transmit};
use tracing::trace;

use crate::udp_port::UdpPort;

/// The transmits of one pass of the thread, kept while it holds the mutex and written to the socket
/// after it lets go, so the frame never waits on a send (15-quic-transport.md, A thread for each
/// endpoint, and no IO in the frame). Its buffers keep their capacity from pass to pass.
#[derive(Debug, Default)]
pub(crate) struct Outbox {
    bytes: Vec<u8>,
    sends: Vec<Outgoing>,
}

/// One transmit: its datagrams' bytes in the outbox, and where they go.
#[derive(Debug)]
struct Outgoing {
    destination: SocketAddr,
    ecn: Option<EcnCodepoint>,
    segment_size: Option<usize>,
    src_ip: Option<IpAddr>,
    bytes: Range<usize>,
}

impl Outbox {
    /// Keeps `transmit`, whose bytes start `buffer`.
    pub(crate) fn push(&mut self, transmit: &Transmit, buffer: &[u8]) {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(&buffer[..transmit.size]);
        self.sends.push(Outgoing {
            destination: transmit.destination,
            ecn: transmit.ecn,
            segment_size: transmit.segment_size,
            src_ip: transmit.src_ip,
            bytes: start..self.bytes.len(),
        });
    }

    /// Writes every transmit to `port`, and empties the outbox. A write that would block drops
    /// its datagrams, which QUIC takes as lost.
    pub(crate) fn send(&mut self, port: &UdpPort) {
        for send in self.sends.drain(..) {
            let transmit = quinn_udp::Transmit {
                destination: send.destination,
                ecn: send.ecn.map(Self::udp_ecn),
                contents: &self.bytes[send.bytes],
                segment_size: send.segment_size,
                src_ip: send.src_ip,
            };
            if let Err(error) = port.send(&transmit) {
                debug_assert_eq!(
                    error.kind(),
                    ErrorKind::WouldBlock,
                    "quinn-udp logs every other error itself"
                );
                trace!("the socket is full; a transmit is dropped");
            }
        }
        self.bytes.clear();
    }

    const fn udp_ecn(ecn: EcnCodepoint) -> quinn_udp::EcnCodepoint {
        match ecn {
            EcnCodepoint::Ect0 => quinn_udp::EcnCodepoint::Ect0,
            EcnCodepoint::Ect1 => quinn_udp::EcnCodepoint::Ect1,
            EcnCodepoint::Ce => quinn_udp::EcnCodepoint::Ce,
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::net::SocketAddr;

    use crate::outbox::Outbox;

    impl Outbox {
        /// Empties the outbox into `deliver`, one transmit at a time, as the socket would send
        /// it: its destination, its bytes, and the length of its datagrams, which GSO splits it
        /// into.
        pub(crate) fn drain(&mut self, mut deliver: impl FnMut(SocketAddr, &[u8], usize)) {
            for send in self.sends.drain(..) {
                let bytes = &self.bytes[send.bytes];
                deliver(
                    send.destination,
                    bytes,
                    send.segment_size.unwrap_or(bytes.len()),
                );
            }
            self.bytes.clear();
        }
    }
}
