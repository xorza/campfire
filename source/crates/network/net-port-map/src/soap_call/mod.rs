use xmltree::{Element, EmitterConfig, Namespace, XMLNode};

use crate::error::SoapError;
use crate::router_xml;

/// SOAP 1.1's envelope and encoding namespaces.
const ENVELOPE: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const ENCODING: &str = "http://schemas.xmlsoap.org/soap/encoding/";

/// One action of a UPnP router's WAN connection service, as SOAP over HTTP carries it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SoapCall<'a> {
    pub(crate) service_type: &'a str,
    pub(crate) action: &'a str,
}

impl SoapCall<'_> {
    /// The value of the request's `SOAPAction` header.
    pub(crate) fn header(&self) -> String {
        format!("\"{}#{}\"", self.service_type, self.action)
    }

    /// The request's envelope, which `xmltree` writes, so the router's service type and each value
    /// are escaped: each of `arguments`, in their order, as the router's schema names them, with
    /// the value `value` gives it. An argument with no value is left out, as `igd-next` left out
    /// one it did not know, so only a name `value` knows becomes an element.
    pub(crate) fn envelope(
        &self,
        arguments: &[String],
        value: impl Fn(&str) -> Option<String>,
    ) -> Vec<u8> {
        let mut call = Self::element(self.action, "u", self.service_type);
        call.children = arguments
            .iter()
            .filter_map(|argument| {
                let mut element = Element::new(argument);
                element.children.push(XMLNode::Text(value(argument)?));
                Some(XMLNode::Element(element))
            })
            .collect();
        let mut body = Self::element("Body", "s", ENVELOPE);
        body.children.push(XMLNode::Element(call));
        let mut envelope = Self::element("Envelope", "s", ENVELOPE);
        envelope
            .attributes
            .insert(String::from("s:encodingStyle"), String::from(ENCODING));
        envelope.children.push(XMLNode::Element(body));
        let mut bytes = Vec::new();
        envelope
            .write_with_config(&mut bytes, EmitterConfig::new())
            .expect("an envelope of elements and text writes into a Vec");
        bytes
    }

    /// An element `name` of the namespace `uri`, which it binds to `prefix`.
    fn element(name: &str, prefix: &str, uri: &str) -> Element {
        let mut namespaces = Namespace::empty();
        namespaces.put(prefix, uri);
        let mut element = Element::new(name);
        element.prefix = Some(String::from(prefix));
        element.namespace = Some(String::from(uri));
        element.namespaces = Some(namespaces);
        element
    }

    /// The action's response in the reply `body`, or the UPnP error code of its fault.
    pub(crate) fn reply(&self, body: &[u8]) -> Result<Element, SoapError> {
        let mut root = router_xml::parse(body).ok_or(SoapError::Reply)?;
        let reply = root.get_mut_child("Body").ok_or(SoapError::Reply)?;
        if let Some(response) = reply.take_child(format!("{}Response", self.action).as_str()) {
            return Ok(response);
        }
        let code = reply
            .get_child("Fault")
            .and_then(|fault| fault.get_child("detail"))
            .and_then(|detail| detail.get_child("UPnPError"))
            .and_then(|error| error.get_child("errorCode"))
            .and_then(Element::get_text)
            .and_then(|code| code.trim().parse().ok())
            .ok_or(SoapError::Reply)?;
        Err(SoapError::Refused(code))
    }
}

#[cfg(test)]
mod tests;
