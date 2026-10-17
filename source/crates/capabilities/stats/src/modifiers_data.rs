use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, Section, SectionTable};
use campfire_script::{NameKind, UnknownName};
use campfire_values::EngineTag;
use serde::Deserialize;

use crate::error::ModifiersDataError;
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
                    let problem = ModifiersDataError::EngineTag(tag.clone());
                    return Err(BookError::rule(package, at(), problem));
                }
                let aura = modifier.aura.as_ref().map(|aura| &aura.modifier);
                if let Some(aura) = aura.filter(|aura| !modifiers.0.contains_key(*aura)) {
                    let name = UnknownName::new(NameKind::Modifier, aura);
                    return Err(BookError::unknown(package, at(), name));
                }
            }
        }
        Ok(())
    }
}
