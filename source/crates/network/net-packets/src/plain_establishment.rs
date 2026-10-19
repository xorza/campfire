use campfire_net_core::Instant;
use core::net::SocketAddr;

use crate::connection_id::ConnectionId;
use crate::datagram_io::Datagram;
use crate::establish::{Establish, Established, Establishing};
use crate::protection::Protection;

/// Plain sessions with no handshake, for an IO in one process: a client's session opens at its
/// connect, and a server takes each peer by its first datagram while it accepts.
#[derive(Debug, Default)]
pub(crate) struct PlainEstablishment {
    accepting: bool,
}

impl PlainEstablishment {
    const fn established(peer: SocketAddr) -> Established {
        Established {
            peer,
            protection: Protection::Plain,
            prefix: None,
            route: None,
            reset_token: None,
            binding: None,
        }
    }
}

impl Establish for PlainEstablishment {
    fn connect(&mut self, _now: Instant, server: SocketAddr, out: &mut Establishing) {
        out.established.push(Self::established(server));
    }

    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) {}

    fn unknown(&mut self, _now: Instant, datagram: &Datagram, out: &mut Establishing) {
        if self.accepting {
            out.established.push(Self::established(datagram.from));
        }
    }

    fn set_accepting(&mut self, accepting: bool) {
        self.accepting = accepting;
    }

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn routes_by_prefix(&self) -> bool {
        false
    }

    /// Never asked: a plain session routes by address, so its peer never moves.
    fn challenge(&mut self) -> [u8; 8] {
        unreachable!("a session routed by address has no candidate path")
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}
