use xmltree::Element;

use crate::router_url::RouterUrl;
use crate::router_xml;

/// The WAN connection services a router maps ports through, as `igd-next` took them.
const WAN_SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:2",
];

/// What a router's device description says of its WAN connection service (11-net-stack/port-map.md,
/// A router is found by its search answer or by its announcement): its type, its control URL and
/// its service schema's, both at the description's address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouterDescription {
    service_type: String,
    control: RouterUrl,
    schema: RouterUrl,
}

impl RouterDescription {
    /// The description `body` at `location` holds: its first WAN connection service, the
    /// devices' tree read depth first, as `igd-next` read it; its URLs read against its
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
                    (url.address().ip() == location.address().ip()).then_some(url)
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
mod tests;
