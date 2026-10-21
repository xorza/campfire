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
