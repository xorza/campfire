use std::net::{Ipv4Addr, SocketAddrV4};

use super::*;

#[test]
fn a_description_gives_its_first_wan_service_depth_first_at_its_own_address() {
    let location = RouterUrl::parse("http://192.168.0.1:1900/igd/desc.xml").unwrap();
    let service = |kind: &str, control: &str| {
        format!(
            "<service><serviceType>urn:schemas-upnp-org:service:{kind}</serviceType>\
                 <SCPDURL>scpd.xml</SCPDURL><controlURL>{control}</controlURL></service>"
        )
    };
    let device = |services: &str, children: &str| {
        format!(
            "<device><serviceList>{services}</serviceList><deviceList>{children}</deviceList></device>"
        )
    };
    let document = |base: &str, body: &str| {
        format!(
            r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0">{base}{body}</root>"#
        )
    };
    // The gateway, a WAN device, and its connection device, whose IP connection is the
    // first WAN service depth first: before the second WAN device's.
    let layer3 = service("Layer3Forwarding:1", "/l3f");
    let first = device("", &device(&service("WANIPConnection:1", "/ctl/ip"), ""));
    let second = device(&service("WANPPPConnection:1", "/ctl/ppp"), "");
    let tree = device(&layer3, &format!("{first}{second}"));
    let parsed = RouterDescription::parse(document("", &tree).as_bytes(), &location).unwrap();
    assert_eq!(
        parsed.service_type(),
        "urn:schemas-upnp-org:service:WANIPConnection:1"
    );
    assert_eq!(parsed.control().whole(), "http://192.168.0.1:1900/ctl/ip");
    assert_eq!(
        parsed.schema().whole(),
        "http://192.168.0.1:1900/igd/scpd.xml"
    );
    // A `URLBase` replaces the location's, at another port of the router.
    let based = document("<URLBase>http://192.168.0.1:5000/</URLBase>", &tree);
    let parsed = RouterDescription::parse(based.as_bytes(), &location).unwrap();
    assert_eq!(
        parsed.control().address(),
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 5000)
    );
    assert_eq!(parsed.schema().whole(), "http://192.168.0.1:5000/scpd.xml");
    // A service at another address, a document with no WAN service, and one that does not
    // read are none.
    let foreign = device(&service("WANIPConnection:1", "http://192.168.0.9/ctl"), "");
    for body in [
        document("", &foreign),
        document("", &device(&layer3, "")),
        String::from("<root><device>"),
    ] {
        assert_eq!(
            RouterDescription::parse(body.as_bytes(), &location),
            None,
            "{body}"
        );
    }
}
