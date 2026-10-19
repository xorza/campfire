use std::net::{IpAddr, SocketAddr};

use crate::router_url::RouterUrl;

/// An SSDP message: HTTP's start line and headers in one datagram, with no body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SsdpHeaders<'a> {
    start: &'a str,
    text: &'a str,
}

impl<'a> SsdpHeaders<'a> {
    /// The message `datagram` holds, when it is text.
    pub(crate) fn parse(datagram: &'a [u8]) -> Option<Self> {
        let text = std::str::from_utf8(datagram).ok()?;
        let start = text.split("\r\n").next()?;
        Some(Self { start, text })
    }

    pub(crate) const fn start(&self) -> &'a str {
        self.start
    }

    /// The value of header `name`, which matches any case, as HTTP's names do.
    pub(crate) fn get(&self, name: &str) -> Option<&'a str> {
        self.text.split("\r\n").skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

    /// The router's description at `LOCATION`, when it is at the address of `from`, the message's
    /// sender (13-net-stack/port-map.md, A router is found by its search answer or by its
    /// announcement).
    pub(crate) fn location_of(&self, from: SocketAddr) -> Option<RouterUrl> {
        let location = RouterUrl::parse(self.get("LOCATION")?)?;
        (from.ip() == IpAddr::V4(*location.addr().ip())).then_some(location)
    }
}
