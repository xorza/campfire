use std::sync::Arc;

use campfire_values::{EngineTag, Names, Tag, TagSet};

/// The match's tags as a filter reads them: each tag's name, the engine's first, and the engine's
/// tags that a filter selects only when it names one.
#[derive(Debug, Clone, Default)]
pub struct TagNames {
    names: Arc<Names<Tag>>,
    named_only: TagSet,
}

impl TagNames {
    /// The engine's tags `engine`, at their places, and no other.
    pub fn new(engine: &[EngineTag]) -> Self {
        let named_only = engine
            .iter()
            .filter(|tag| tag.properties().is_named_only())
            .map(|tag| tag.tag())
            .collect();
        Self {
            names: Arc::new(engine.iter().map(|tag| tag.name()).collect()),
            named_only,
        }
    }

    /// The tags' names.
    pub const fn names(&self) -> &Arc<Names<Tag>> {
        &self.names
    }

    /// The tags' names, to add one to.
    pub fn names_mut(&mut self) -> &mut Names<Tag> {
        Arc::make_mut(&mut self.names)
    }

    /// The tag named `name`.
    pub fn named(&self, name: &str) -> Option<Tag> {
        self.names.named(name)
    }

    /// The engine's tags that a filter selects only when it names one.
    pub const fn named_only(&self) -> TagSet {
        self.named_only
    }
}
