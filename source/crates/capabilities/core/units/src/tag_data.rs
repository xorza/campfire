use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use campfire_values::{Block, TagProperties};
use serde::Deserialize;

/// A tag's properties as the mode's `[tags.<name>]` declares them; a tag it does not declare has
/// none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagData {
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub detects: bool,
    /// The tags whose modifiers it holds without effect.
    #[serde(default)]
    pub immune: Vec<DeclaredName>,
}

impl TagData {
    /// The properties it gives its tag.
    pub fn properties(&self) -> TagProperties {
        let blocks = self
            .blocks
            .iter()
            .fold(TagProperties::default(), |properties, &block| {
                properties.with_block(block)
            });
        let hidden = if self.hidden {
            blocks.with_hidden()
        } else {
            blocks
        };
        if self.detects {
            hidden.with_detects()
        } else {
            hidden
        }
    }
}

impl DataSection for TagData {
    const TABLE: DataTable = DataTable::new("A tag's properties, `[tags.<name>]`");
}

/// The mode's `[tags]`: the properties of its tags, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModeTagsData(pub BTreeMap<DeclaredName, TagData>);

impl Section for ModeTagsData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "tags";
}
