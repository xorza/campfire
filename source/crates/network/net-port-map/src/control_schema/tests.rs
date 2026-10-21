use super::*;

#[test]
fn a_schema_gives_each_actions_arguments_in_in_order() {
    let body = r#"<?xml version="1.0"?><scpd xmlns="urn:schemas-upnp-org:service-1-0"><actionList>
            <action><name>AddPortMapping</name><argumentList>
                <argument><name>NewRemoteHost</name><direction>in</direction></argument>
                <argument><name>NewExternalPort</name><direction>in</direction></argument>
            </argumentList></action>
            <action><name>GetExternalIPAddress</name><argumentList>
                <argument><name>NewExternalIPAddress</name><direction>out</direction></argument>
            </argumentList></action>
            <action><name>NoArguments</name></action>
        </actionList></scpd>"#;
    let schema = ControlSchema::parse(body.as_bytes()).unwrap();
    let add = ["NewRemoteHost", "NewExternalPort"].map(String::from);
    assert_eq!(schema.arguments("AddPortMapping"), Some(add.as_slice()));
    assert_eq!(
        schema.arguments("GetExternalIPAddress"),
        Some([].as_slice())
    );
    // An action with no argument list is left out, and one the schema does not name is none.
    assert_eq!(schema.arguments("NoArguments"), None);
    assert_eq!(schema.arguments("DeletePortMapping"), None);
    assert_eq!(ControlSchema::parse(b"<scpd></scpd>"), None);
}
