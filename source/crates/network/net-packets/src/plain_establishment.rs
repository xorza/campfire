use core::net::SocketAddr;

use campfire_net_core::Instant;

use crate::connection_id::ConnectionId;
use crate::datagram_io::Datagram;
use crate::establish::{
    ClientEstablish, Establish, Established, Establishing, PathCheck, ReadCost, ServerEstablish,
};
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
            local: None,
            protection: Protection::Plain,
            prefix: None,
            route: None,
            reset_token: None,
            binding: None,
            early: Vec::new(),
        }
    }
}

impl Establish for PlainEstablishment {
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) -> ReadCost {
        ReadCost::Light
    }

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
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

    /// None: a plain packet session routes by address, so its peer never moves.
    fn path_check(&mut self) -> Option<&mut dyn PathCheck> {
        None
    }
}
