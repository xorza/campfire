use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Section, SectionTable, StateFields};
use campfire_script::UnknownName;
use campfire_units::DeclaredTags;
use campfire_values::{EngineTag, ModifierId, Named};
use serde::Deserialize;

use crate::error::ModifierDataError;
use crate::modifier_data::ModifierData;

/// A package's `[modifiers]`: its modifiers, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModifiersData(pub BTreeMap<DeclaredName, ModifierData>);

impl Section for ModifiersData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "modifiers";
}

impl ModifiersData {
    /// Adds the tags every package's modifiers grant.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn granted_tags(cx: &BookCx<'_>, tags: &mut DeclaredTags) -> Result<(), BookError> {
        for (_, source) in cx.packages() {
            let modifiers = source.content.get::<Self>();
            for modifier in modifiers
                .into_iter()
                .flat_map(|modifiers| modifiers.0.values())
            {
                tags.extend(&modifier.tags);
            }
        }
        Ok(())
    }

    /// Adds the state fields of every package's modifiers.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn state_fields(cx: &BookCx<'_>, fields: &mut StateFields) -> Result<(), BookError> {
        for (_, source) in cx.packages() {
            let modifiers = source.content.get::<Self>();
            for modifier in modifiers
                .into_iter()
                .flat_map(|modifiers| modifiers.0.values())
            {
                fields.extend(modifier.state.keys());
            }
        }
        Ok(())
    }

    /// The rule of every package's modifiers: each grants no tag only the engine gives, and an
    /// aura's modifier is one of its package's, through which the ways that apply it pass on.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let engine = cx.source().engine_tags;
        for (package, source) in cx.packages() {
            let Some(modifiers) = source.content.get::<Self>() else {
                continue;
            };
            for (id, modifier) in &modifiers.0 {
                let at = || BookPlace::Entry {
                    table: Self::KEY,
                    name: id.clone(),
                };
                let mut tags = modifier.tags.iter();
                if let Some(tag) = tags.find(|tag| EngineTag::named(engine, tag.as_str()).is_some())
                {
                    let problem = ModifierDataError::EngineTag(tag.clone());
                    return Err(BookError::rule(package, at(), problem));
                }
                let aura = modifier.aura.as_ref().map(|aura| &aura.modifier);
                if let Some(aura) = aura.filter(|aura| !modifiers.0.contains_key(*aura)) {
                    let name = UnknownName::new(ModifierId::NAMES, aura);
                    return Err(BookError::unknown(package, at(), name));
                }
            }
        }
        Ok(())
    }
}
