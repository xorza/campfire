use campfire_net_core::Instant;
use core::net::SocketAddr;

use crate::connection_id::ConnectionId;
use crate::datagram_io::Datagram;
use crate::establish::{ClientEstablish, Establish, Established, Establishing, ServerEstablish};
use crate::protection::Protection;

/// Plain packet sessions with no handshake, for an IO in one process: a client's packet session
/// opens at its connect, and a server takes each peer by its first datagram while it accepts.
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
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) {}

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn routes_by_prefix(&self) -> bool {
        false
    }

    /// Never asked: a plain packet session routes by address, so its peer never moves.
    fn challenge(&mut self) -> [u8; 8] {
        unreachable!("a packet session routed by address has no candidate path")
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ClientEstablish for PlainEstablishment {
    fn connect(&mut self, _now: Instant, server: SocketAddr, out: &mut Establishing) {
        out.established.push(Self::established(server));
    }
}

impl ServerEstablish for PlainEstablishment {
    fn unknown(&mut self, _now: Instant, datagram: &Datagram, out: &mut Establishing) {
        if self.accepting {
            out.established.push(Self::established(datagram.from));
        }
    }

    fn set_accepting(&mut self, accepting: bool) {
        self.accepting = accepting;
    }
}
