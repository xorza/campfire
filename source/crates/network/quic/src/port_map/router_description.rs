use xmltree::Element;

use crate::port_map::router_url::RouterUrl;
use crate::port_map::router_xml;

/// The WAN connection services a router maps ports through, as `igd-next` takes them.
const WAN_SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:2",
];

/// What a router's device description says of its WAN connection service (11-quic-transport.md, A
/// server maps its own port): its type, its control URL and its service schema's, both at the
/// description's address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouterDescription {
    service_type: String,
    control: RouterUrl,
    schema: RouterUrl,
}

impl RouterDescription {
    /// The description `body` at `location` holds: its first WAN connection service, the
    /// devices' tree read depth first, as `igd-next` reads it; its URLs read against its
    /// `URLBase`, else its location. None for a document that does not read, holds no such
    /// service, or names a URL at another address.
    pub(crate) fn parse(body: &[u8], location: &RouterUrl) -> Option<Self> {
        let root = router_xml::parse(body)?;
        let base = match root.get_child("URLBase").and_then(Element::get_text) {
            Some(base) => location.join(&base)?,
            None => location.clone(),
        };
        let mut pending: Vec<&Element> = router_xml::children(&root, "device").collect();
        pending.reverse();
        while let Some(device) = pending.pop() {
            let services = device.get_child("serviceList").into_iter();
            let services = services.flat_map(|list| router_xml::children(list, "service"));
            for service in services {
                let Some(service_type) =
                    service.get_child("serviceType").and_then(Element::get_text)
                else {
                    continue;
                };
                if !WAN_SERVICES.contains(&service_type.as_ref()) {
                    continue;
                }
                let url = |name: &str| {
                    let url = base.join(&service.get_child(name)?.get_text()?)?;
                    (url.addr().ip() == location.addr().ip()).then_some(url)
                };
                let (control, schema) = (url("controlURL")?, url("SCPDURL")?);
                return Some(Self {
                    service_type: service_type.into_owned(),
                    control,
                    schema,
                });
            }
            let devices = device.get_child("deviceList").into_iter();
            let mut children: Vec<&Element> = devices
                .flat_map(|list| router_xml::children(list, "device"))
                .collect();
            children.reverse();
            pending.extend(children);
        }
        None
    }

    pub(crate) fn service_type(&self) -> &str {
        &self.service_type
    }

    pub(crate) const fn control(&self) -> &RouterUrl {
        &self.control
    }

    pub(crate) const fn schema(&self) -> &RouterUrl {
        &self.schema
    }
}

#[cfg(test)]
mod tests {
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
            parsed.control().addr(),
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
}
