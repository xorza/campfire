use xmltree::Element;

use crate::router_xml;

/// A WAN connection service's schema: each action's name, with the names of its arguments in, in
/// order, by which a request names its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlSchema(Vec<(String, Vec<String>)>);

impl ControlSchema {
    /// The schema `body` holds: the actions of its first `actionList`, each with a name and an
    /// `argumentList`, as `igd-next` read them; none for a document that does not read or holds
    /// no list.
    pub(crate) fn parse(body: &[u8]) -> Option<Self> {
        let root = router_xml::parse(body)?;
        let list = router_xml::children(&root, "actionList").next()?;
        let actions = router_xml::children(list, "action").filter_map(|action| {
            let name = action.get_child("name")?.get_text()?.into_owned();
            let arguments = action.get_child("argumentList")?;
            let ins = router_xml::children(arguments, "argument").filter_map(Self::input);
            Some((name, ins.collect()))
        });
        Some(Self(actions.collect()))
    }

    /// The name of `argument` when it goes in.
    fn input(argument: &Element) -> Option<String> {
        let direction = argument.get_child("direction")?.get_text()?;
        if direction != "in" {
            return None;
        }
        Some(argument.get_child("name")?.get_text()?.into_owned())
    }

    /// The arguments in of `action`, when the service offers it.
    pub(crate) fn arguments(&self, action: &str) -> Option<&[String]> {
        self.0
            .iter()
            .find(|(name, _)| name == action)
            .map(|(_, arguments)| arguments.as_slice())
    }
}

#[cfg(test)]
mod tests;
