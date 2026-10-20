use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::router_url::RouterUrl;
use crate::ssdp_headers::SsdpHeaders;

/// What a search asks for, one search each: a router that maps ports by UPnP-IGD, as its gateway
/// device or its WAN connection service, as `igd-next` asked.
const TARGETS: [&str; 3] = [
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
];
/// The longest a router waits before it answers, in seconds, which spreads the answers of many.
const MAX_WAIT: u8 = 3;
/// The largest answer read, a datagram that fits an Ethernet frame.
const ANSWER_MAX: usize = 1500;

/// An SSDP search for a UPnP-IGD router (13-net-stack/port-map.md, A router is found by its search
/// answer or by its announcement), from a socket of its own, which reads answers until its
/// deadline.
#[derive(Debug)]
pub(crate) struct SsdpSearch {
    socket: UdpSocket,
    deadline: Instant,
}

impl SsdpSearch {
    /// Sends the search for each target to `ssdp`, whose answers come within `timeout`. It fails
    /// only when no search went.
    pub(crate) fn send(ssdp: SocketAddr, timeout: Duration) -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        let mut failed = None;
        let mut sent = false;
        for target in TARGETS {
            let request = format!(
                "M-SEARCH * HTTP/1.1\r\nHOST: {ssdp}\r\nMAN: \"ssdp:discover\"\r\nMX: {MAX_WAIT}\r\nST: \
                 {target}\r\n\r\n"
            );
            match socket.send_to(request.as_bytes(), ssdp) {
                Ok(_) => sent = true,
                Err(error) => failed = Some(error),
            }
        }
        if let (false, Some(error)) = (sent, failed) {
            return Err(error);
        }
        Ok(Self {
            socket,
            deadline: Instant::now() + timeout,
        })
    }

    /// What is left of the search's time.
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// The location of the next router that answers before the deadline: an answer of success
    /// whose location is at its sender's address. None once the deadline passed.
    pub(crate) fn next_location(&self) -> Option<RouterUrl> {
        let mut buffer = [0; ANSWER_MAX];
        loop {
            let remaining = self.remaining();
            if remaining.is_zero() {
                return None;
            }
            self.socket.set_read_timeout(Some(remaining)).ok()?;
            let (read, from) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    return None;
                }
                // An error of one datagram, as an ICMP error that Windows reports on the next read,
                // ends no search.
                Err(_) => continue,
            };
            if let Some(location) = Self::answer(&buffer[..read], from) {
                return Some(location);
            }
        }
    }

    /// The router's location in `datagram`, which came from `from`, when it is a search's answer of
    /// success at its sender's address.
    fn answer(datagram: &[u8], from: SocketAddr) -> Option<RouterUrl> {
        SsdpHeaders::of_success(datagram)?.location_of(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A search's answer of success gives its location at its sender's address; an answer of
    /// failure, a location at another address, an announcement and a non-text datagram are none.
    #[test]
    fn an_answer_of_success_at_its_senders_address_gives_its_location() {
        let router = SocketAddr::from((Ipv4Addr::new(192, 168, 0, 1), 1900));
        let answer = |start: &str, location: &str| {
            format!("{start}\r\nST: upnp:rootdevice\r\nlocation: {location}\r\n\r\n").into_bytes()
        };
        let own = "http://192.168.0.1:5000/desc.xml";
        assert_eq!(
            SsdpSearch::answer(&answer("HTTP/1.1 200 OK", own), router)
                .map(|location| location.whole())
                .as_deref(),
            Some(own)
        );
        for datagram in [
            answer("HTTP/1.1 404 Not Found", own),
            answer("HTTP/1.1 200 OK", "http://192.168.0.9:5000/desc.xml"),
            answer("NOTIFY * HTTP/1.1", own),
            vec![0xff, 0xfe],
        ] {
            assert_eq!(SsdpSearch::answer(&datagram, router), None);
        }
    }
}
