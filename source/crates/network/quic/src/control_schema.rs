use xmltree::Element;

use crate::router_xml;

/// A WAN connection service's schema as `igd-next`'s `Gateway` takes it: each action's name,
/// with the names of its arguments in, in order, by which a request names its values. The
/// `Gateway` holds it in a map of its own type, which it collects into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlSchema(Vec<(String, Vec<String>)>);

impl ControlSchema {
    /// The schema `body` holds: the actions of its first `actionList`, each with a name and an
    /// `argumentList`, as `igd-next` reads them; none for a document that does not read or holds
    /// no list.
    pub(crate) fn parse(body: &[u8]) -> Option<ControlSchema> {
        let root = router_xml::parse(body)?;
        let list = router_xml::children(&root, "actionList").next()?;
        let actions = router_xml::children(list, "action").filter_map(|action| {
            let name = action.get_child("name")?.get_text()?.into_owned();
            let arguments = action.get_child("argumentList")?;
            let ins = router_xml::children(arguments, "argument").filter_map(ControlSchema::input);
            Some((name, ins.collect()))
        });
        Some(ControlSchema(actions.collect()))
    }

    /// The name of `argument` when it goes in.
    fn input(argument: &Element) -> Option<String> {
        let direction = argument.get_child("direction")?.get_text()?;
        if direction != "in" {
            return None;
        }
        Some(argument.get_child("name")?.get_text()?.into_owned())
    }

    /// Each action, with its arguments in.
    pub(crate) fn into_actions(self) -> impl Iterator<Item = (String, Vec<String>)> {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
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
        let schema: Vec<_> = ControlSchema::parse(body.as_bytes())
            .unwrap()
            .into_actions()
            .collect();
        let add = ["NewRemoteHost", "NewExternalPort"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            schema,
            [
                (String::from("AddPortMapping"), add),
                (String::from("GetExternalIPAddress"), Vec::new()),
            ]
        );
        assert_eq!(ControlSchema::parse(b"<scpd></scpd>"), None);
    }
}
