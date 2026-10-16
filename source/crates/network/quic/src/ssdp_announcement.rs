use std::net::{IpAddr, SocketAddr};

use crate::router_url::RouterUrl;

/// The notification types of a router that maps ports by UPnP-IGD: the gateway device, or its
/// WAN connection service.
const GATEWAY_TYPES: [&str; 5] = [
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:2",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
];

/// A UPnP-IGD router's `NOTIFY` announcement that it is alive, multicast to SSDP's group (design
/// 15, D12): where its description is. Its location's address is the announcer's, as `igd-next`
/// trusts a search's answer only at its sender's address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SsdpAnnouncement {
    location: RouterUrl,
}

impl SsdpAnnouncement {
    /// The announcement `datagram` holds, sent from `from`: none for anything but a gateway's
    /// `ssdp:alive` at a location of its sender's address.
    pub(crate) fn parse(datagram: &[u8], from: SocketAddr) -> Option<Self> {
        let text = std::str::from_utf8(datagram).ok()?;
        let start = text.split("\r\n").next()?;
        if !start.eq_ignore_ascii_case("NOTIFY * HTTP/1.1") {
            return None;
        }
        let header = |name: &str| {
            text.split("\r\n").skip(1).find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
            })
        };
        let alive = header("NTS").is_some_and(|nts| nts.eq_ignore_ascii_case("ssdp:alive"));
        let gateway = header("NT").is_some_and(|nt| GATEWAY_TYPES.contains(&nt));
        let location = RouterUrl::parse(header("LOCATION")?)?;
        let own = from.ip() == IpAddr::V4(*location.addr().ip());
        (alive && gateway && own).then_some(Self { location })
    }

    pub(crate) const fn location(&self) -> &RouterUrl {
        &self.location
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn notify(nt: &str, nts: &str, location: &str) -> Vec<u8> {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\n\
             location: {location}\r\nNT: {nt}\r\nNTS: {nts}\r\nUSN: uuid:igd::{nt}\r\n\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn a_gateways_alive_announcement_at_its_own_address_gives_its_location() {
        let router = SocketAddr::from((Ipv4Addr::new(192, 168, 0, 1), 41_952));
        let igd = "urn:schemas-upnp-org:device:InternetGatewayDevice:1";
        let location = "http://192.168.0.1:1900/igd1.xml";
        let heard = SsdpAnnouncement::parse(&notify(igd, "ssdp:alive", location), router);
        assert_eq!(
            heard.map(|heard| heard.location().whole()).as_deref(),
            Some(location)
        );
        let wan = "urn:schemas-upnp-org:service:WANIPConnection:1";
        assert!(SsdpAnnouncement::parse(&notify(wan, "ssdp:alive", location), router).is_some());
        // A goodbye, a device of another type, a location at another address, and a search's
        // answer are none.
        let printer = "urn:schemas-upnp-org:device:Printer:1";
        let elsewhere = "http://192.168.0.9:1900/igd1.xml";
        for datagram in [
            notify(igd, "ssdp:byebye", location),
            notify(printer, "ssdp:alive", location),
            notify(igd, "ssdp:alive", elsewhere),
            b"HTTP/1.1 200 OK\r\nLOCATION: http://192.168.0.1:1900/igd1.xml\r\n\r\n".to_vec(),
        ] {
            assert_eq!(SsdpAnnouncement::parse(&datagram, router), None);
        }
    }
}
