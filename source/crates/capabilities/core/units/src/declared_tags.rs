use std::collections::BTreeSet;

use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, Fact};
use campfire_values::EngineTag;

use crate::tag_data::ModeTagsData;
use crate::unit_type_core::TypeTagsData;

/// Every tag the packages name, each once, sorted: those of every unit type, of the mode's
/// `[tags]` and their immunities, and those the modules whose data names a tag add, as a
/// modifier's grants and a navigation layer. A match declares them in this order after the
/// engine's, so it numbers them the same however it loads.
#[derive(Debug, Default)]
pub struct DeclaredTags(BTreeSet<DeclaredName>);

impl Fact for DeclaredTags {}

impl DeclaredTags {
    /// The tags of the unit types and of the mode's `[tags]`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mut tags = Self::default();
        for unit in cx.types() {
            if let Some(held) = unit.data.section::<TypeTagsData>() {
                tags.extend(&held.0);
            }
        }
        for (name, tag) in cx
            .mode_section::<ModeTagsData>()
            .into_iter()
            .flat_map(|tags| &tags.0)
        {
            tags.extend([name].into_iter().chain(&tag.immune));
        }
        Ok(tags)
    }

    /// Adds the tags `names`.
    pub fn extend<'n>(&mut self, names: impl IntoIterator<Item = &'n DeclaredName>) {
        self.0.extend(names.into_iter().cloned());
    }

    /// The tags but those of `engine`, which the engine declares first, in their order.
    pub fn of_packages<'t>(
        &'t self,
        engine: &'t [EngineTag],
    ) -> impl Iterator<Item = &'t DeclaredName> {
        (self.0.iter()).filter(|name| EngineTag::named(engine, name.as_str()).is_none())
    }
}
