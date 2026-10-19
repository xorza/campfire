use std::fmt::Write as _;

use xmltree::Element;

use crate::error::SoapError;
use crate::router_xml;

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

    /// The request's envelope: each of `arguments`, in their order, as the router's schema names
    /// them, with the value `value` gives it. An argument with no value is left out, as `igd-next`
    /// left out one it did not know.
    pub(crate) fn envelope(
        &self,
        arguments: &[String],
        value: impl Fn(&str) -> Option<String>,
    ) -> String {
        let Self {
            service_type,
            action,
        } = self;
        let mut body = String::new();
        for argument in arguments {
            if let Some(value) = value(argument) {
                write!(body, "<{argument}>{}</{argument}>", escape(&value))
                    .expect("a String takes every write");
            }
        }
        format!(
            r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{service_type}">{body}</u:{action}></s:Body></s:Envelope>"#
        )
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

/// `text` with XML's five special characters escaped, so that an argument's value holds no markup.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALL: SoapCall<'static> = SoapCall {
        service_type: "urn:schemas-upnp-org:service:WANIPConnection:1",
        action: "AddPortMapping",
    };

    fn envelope(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>{body}</s:Body></s:Envelope>"#
        )
    }

    /// The header names the service and the action; the envelope gives the arguments in the
    /// schema's order, leaves out one with no value, and escapes a value's markup.
    #[test]
    fn a_call_names_its_action_and_gives_its_arguments_in_the_schemas_order() {
        assert_eq!(
            CALL.header(),
            "\"urn:schemas-upnp-org:service:WANIPConnection:1#AddPortMapping\""
        );
        let arguments =
            ["NewProtocol", "NewUnknown", "NewPortMappingDescription"].map(String::from);
        let request = CALL.envelope(&arguments, |argument| match argument {
            "NewProtocol" => Some(String::from("UDP")),
            "NewPortMappingDescription" => Some(String::from("a & b </x><evil>")),
            _ => None,
        });
        let body = r#"<u:AddPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1"><NewProtocol>UDP</NewProtocol><NewPortMappingDescription>a &amp; b &lt;/x&gt;&lt;evil&gt;</NewPortMappingDescription></u:AddPortMapping>"#;
        assert!(request.contains(body), "{request}");
    }

    /// A reply gives the action's response, or its fault's UPnP error code; a reply with neither,
    /// a fault with no code, and a body that does not read are no answer.
    #[test]
    fn a_reply_gives_the_response_or_the_faults_code() {
        let response = envelope(
            r#"<u:AddPortMappingResponse xmlns:u="urn:x"><NewReservedPort>40000</NewReservedPort></u:AddPortMappingResponse>"#,
        );
        let reply = CALL.reply(response.as_bytes()).unwrap();
        assert_eq!(
            reply
                .get_child("NewReservedPort")
                .and_then(Element::get_text)
                .as_deref(),
            Some("40000")
        );
        let fault = |code: &str| {
            envelope(&format!(
                r#"<s:Fault><detail><UPnPError xmlns="urn:schemas-upnp-org:control-1-0"><errorCode>{code}</errorCode></UPnPError></detail></s:Fault>"#
            ))
        };
        assert!(matches!(
            CALL.reply(fault(" 718 ").as_bytes()),
            Err(SoapError::Refused(718))
        ));
        for body in [
            envelope(r#"<u:DeletePortMappingResponse xmlns:u="urn:x"/>"#),
            fault("none"),
            String::from("<s:Envelope>"),
        ] {
            assert!(
                matches!(CALL.reply(body.as_bytes()), Err(SoapError::Reply)),
                "{body}"
            );
        }
    }
}
