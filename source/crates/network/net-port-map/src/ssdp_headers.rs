use std::net::{IpAddr, SocketAddr};

use httparse::{EMPTY_HEADER, Header, Request, Response, Status};

use crate::router_url::RouterUrl;

/// The most headers an SSDP message holds; a message of more is refused.
const MAX_HEADERS: usize = 32;

/// The headers of an SSDP message: HTTP/1.1's start line and headers in one datagram, with no body,
/// read by `httparse`, which also takes lines that end in a bare LF.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SsdpHeaders<'a> {
    headers: [Header<'a>; MAX_HEADERS],
    len: usize,
}

impl<'a> SsdpHeaders<'a> {
    /// The headers of the search answer of success, `HTTP/1.1 200`, that `datagram` holds.
    pub(crate) fn of_success(datagram: &'a [u8]) -> Option<Self> {
        let mut headers = [EMPTY_HEADER; MAX_HEADERS];
        let mut response = Response::new(&mut headers);
        let Ok(Status::Complete(_)) = response.parse(datagram) else {
            return None;
        };
        let success = response.version == Some(1) && response.code == Some(200);
        let len = response.headers.len();
        success.then_some(Self { headers, len })
    }

    /// The headers of the `NOTIFY * HTTP/1.1` that `datagram` holds.
    pub(crate) fn of_notify(datagram: &'a [u8]) -> Option<Self> {
        let mut headers = [EMPTY_HEADER; MAX_HEADERS];
        let mut request = Request::new(&mut headers);
        let Ok(Status::Complete(_)) = request.parse(datagram) else {
            return None;
        };
        let notify = request.method == Some("NOTIFY")
            && request.path == Some("*")
            && request.version == Some(1);
        let len = request.headers.len();
        notify.then_some(Self { headers, len })
    }

    /// The value of header `name`, which matches any case, as HTTP's names do; none when it is not
    /// UTF-8.
    pub(crate) fn get(&self, name: &str) -> Option<&'a str> {
        self.headers[..self.len]
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .and_then(|header| std::str::from_utf8(header.value).ok())
            .map(str::trim)
    }

    /// The router's description at `LOCATION`, when it is at the address of `from`, the message's
    /// sender (11-net-stack/port-map.md, A router is found by its search answer or by its
    /// announcement).
    pub(crate) fn location_of(&self, from: SocketAddr) -> Option<RouterUrl> {
        let location = RouterUrl::parse(self.get("LOCATION")?)?;
        (from.ip() == IpAddr::V4(*location.address().ip())).then_some(location)
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    /// A message of lines that end in a bare LF reads as one of CRLF; a header of another case is
    /// found; an unfinished message, a second line of status or method, an HTTP/1.0 one and a
    /// message of more headers than the bound are none.
    #[test]
    fn a_message_reads_by_http_s_rules() {
        let router = SocketAddr::from((Ipv4Addr::new(192, 168, 0, 1), 1900));
        let location = "http://192.168.0.1:5000/desc.xml";
        let bare = format!("HTTP/1.1 200 OK\nST: upnp:rootdevice\nLocation:  {location} \n\n");
        let answer = SsdpHeaders::of_success(bare.as_bytes()).unwrap();
        assert_eq!(answer.get("st"), Some("upnp:rootdevice"));
        assert_eq!(
            answer.location_of(router).map(|url| url.whole()).as_deref(),
            Some(location)
        );
        let notify = "NOTIFY * HTTP/1.1\r\nNTS: ssdp:alive\r\n\r\n";
        assert_eq!(
            SsdpHeaders::of_notify(notify.as_bytes())
                .unwrap()
                .get("NTS"),
            Some("ssdp:alive")
        );
        let many = "X-Header: 1\r\n".repeat(MAX_HEADERS + 1);
        let many = format!("HTTP/1.1 200 OK\r\n{many}\r\n");
        let refused = [
            SsdpHeaders::of_success(b"HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\n"),
            SsdpHeaders::of_success(b"HTTP/1.1 2000 OK\r\n\r\n"),
            SsdpHeaders::of_success(b"HTTP/1.0 200 OK\r\n\r\n"),
            SsdpHeaders::of_success(many.as_bytes()),
            SsdpHeaders::of_notify(b"notify * HTTP/1.1\r\n\r\n"),
            SsdpHeaders::of_notify(b"NOTIFY / HTTP/1.1\r\n\r\n"),
        ];
        for (at, read) in refused.iter().enumerate() {
            assert!(read.is_none(), "{at}");
        }
    }
}
