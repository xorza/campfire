use crate::tag::Tag;
use crate::tag_properties::TagProperties;

/// A tag the engine gives and reads, which the module that gives it declares: its number, the same
/// in every match, as the module list places the engine's tags first, in its order; its name,
/// which packages name in filters and in the mode's `[tags]`, though no unit type or modifier of
/// theirs carries it; and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineTag {
    tag: Tag,
    name: &'static str,
    properties: TagProperties,
}

impl EngineTag {
    /// The engine tag of number `number`, which its place among the declared ones must be.
    pub const fn new(number: u8, name: &'static str, properties: TagProperties) -> EngineTag {
        EngineTag {
            tag: Tag::new(number),
            name,
            properties,
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn tag(self) -> Tag {
        self.tag
    }

    /// What it does, beside what a filter reads of it.
    pub const fn properties(self) -> TagProperties {
        self.properties
    }

    /// The tag of `tags`, the release's engine tags, named `name`, if one is.
    pub fn named(tags: &[EngineTag], name: &str) -> Option<EngineTag> {
        tags.iter().copied().find(|tag| tag.name == name)
    }
}
