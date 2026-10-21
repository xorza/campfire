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
/// schema's order, leaves out one with no value, and escapes the markup of a value and of the
/// service type.
#[test]
fn a_call_names_its_action_and_gives_its_arguments_in_the_schemas_order() {
    assert_eq!(
        CALL.header(),
        "\"urn:schemas-upnp-org:service:WANIPConnection:1#AddPortMapping\""
    );
    let arguments = ["NewProtocol", "NewUnknown", "NewPortMappingDescription"].map(String::from);
    let request = String::from_utf8(CALL.envelope(&arguments, |argument| match argument {
        "NewProtocol" => Some(String::from("UDP")),
        "NewPortMappingDescription" => Some(String::from("a & b </x><evil>")),
        _ => None,
    }))
    .unwrap();
    let whole = r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:AddPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1"><NewProtocol>UDP</NewProtocol><NewPortMappingDescription>a &amp; b &lt;/x&gt;&lt;evil&gt;</NewPortMappingDescription></u:AddPortMapping></s:Body></s:Envelope>"#;
    assert_eq!(request, whole);
    // A router's service type with markup stays the namespace's value.
    let odd = SoapCall {
        service_type: r#"urn:x"><evil"#,
        ..CALL
    };
    let request = String::from_utf8(odd.envelope(&[], |_| None)).unwrap();
    assert!(
        request.contains(r#"xmlns:u="urn:x&quot;&gt;&lt;evil""#),
        "{request}"
    );
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
