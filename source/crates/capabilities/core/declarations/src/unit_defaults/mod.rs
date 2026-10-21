use campfire_codec::{TomlTable, TomlValue};

use crate::error::DefaultsError;
use crate::section::SectionTable;
use crate::section::section_readers::SectionReaders;

/// The defaults a mode gives the fields of its unit types' sections, `[defaults.<section>]`: each
/// lies under the section of each unit type that gives it, before the type reads it, so the
/// type's own field stays and a type of no such section takes none (03-game-scripting.md,
/// Defaults under each unit type).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitDefaults(TomlTable);

impl UnitDefaults {
    /// The defaults `value` gives, a table of sections of unit types, each a table of fields its
    /// section reads, as `readers` register them: an error for any other key.
    pub fn read(value: TomlValue, readers: &SectionReaders) -> Result<Self, DefaultsError> {
        let table = value
            .read::<TomlTable>()
            .map_err(|_| DefaultsError::NotSections)?;
        for (section, given) in table.clone().into_entries() {
            let fields = readers.fields(SectionTable::UnitType, &section);
            let fields = fields.ok_or_else(|| DefaultsError::NoSection(section.clone()))?;
            let given = given.read::<TomlTable>();
            let given = given
                .ok()
                .filter(|_| !fields.is_empty())
                .ok_or_else(|| DefaultsError::NotFields(section.clone()))?;
            if let Some(field) = given.keys().find(|field| !fields.contains(field)) {
                let field = field.to_owned();
                return Err(DefaultsError::NoField { section, field });
            }
        }
        Ok(Self(table))
    }

    /// Lays each default under the section of `unit`, a unit type's table, that gives it.
    pub fn lay_under(&self, unit: &mut TomlTable) {
        unit.fill_held(&self.0);
    }
}

#[cfg(test)]
mod tests;
