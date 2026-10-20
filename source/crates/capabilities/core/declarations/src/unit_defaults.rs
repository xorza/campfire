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
mod tests {
    use campfire_codec::Toml;
    use serde::Deserialize;

    use super::*;
    use crate::section::Section;

    /// A unit type's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Turret {
        tolerance: i64,
        speed: Option<i64>,
    }

    impl Section for Turret {
        const TABLE: SectionTable = SectionTable::UnitType;
        const KEY: &'static str = "turret";
    }

    /// A unit type's section of a test that reads no struct.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct Tags(Vec<String>);

    impl Section for Tags {
        const TABLE: SectionTable = SectionTable::UnitType;
        const KEY: &'static str = "tags";
    }

    /// A map's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct Ground {
        cell: i64,
    }

    impl Section for Ground {
        const TABLE: SectionTable = SectionTable::Map;
        const KEY: &'static str = "ground";
    }

    fn read(text: &str) -> Result<UnitDefaults, DefaultsError> {
        let mut readers = SectionReaders::default();
        readers.add::<Turret>(None);
        readers.add::<Tags>(None);
        readers.add::<Ground>(None);
        let mut file: TomlTable = Toml::parse(text).unwrap();
        UnitDefaults::read(file.take_value("defaults").unwrap(), &readers)
    }

    #[test]
    fn a_default_lies_under_a_section_a_unit_type_gives_and_its_own_field_stays() {
        let defaults = read("[defaults.turret]\ntolerance = 5\nspeed = 2").unwrap();
        let unit = |text: &str| {
            let mut unit: TomlTable = Toml::parse(text).unwrap();
            defaults.lay_under(&mut unit);
            unit
        };
        let expected = |text: &str| Toml::parse::<TomlTable>(text).unwrap();
        // Its own `tolerance` stays, and the default `speed` joins it.
        assert_eq!(
            unit("turret = { tolerance = 9 }"),
            expected("turret = { tolerance = 9, speed = 2 }")
        );
        // A type that gives no turret takes none.
        assert_eq!(unit("tags = [\"a\"]"), expected("tags = [\"a\"]"));
        // No defaults change no type.
        let none = UnitDefaults::default();
        let mut given: TomlTable = Toml::parse("turret = {}").unwrap();
        none.lay_under(&mut given);
        assert_eq!(given, expected("turret = {}"));
    }

    #[test]
    fn a_default_of_no_section_of_a_unit_type_or_no_field_of_its_section_fails() {
        assert_eq!(
            read("[defaults.ground]\ncell = 2").unwrap_err(),
            DefaultsError::NoSection("ground".to_owned())
        );
        assert_eq!(
            read("[defaults.turret]\nrange = 2").unwrap_err(),
            DefaultsError::NoField {
                section: "turret".to_owned(),
                field: "range".to_owned()
            }
        );
        // A section that reads no fields by name, and a value no table, take no default.
        assert_eq!(
            read("[defaults]\ntags = [\"a\"]").unwrap_err(),
            DefaultsError::NotFields("tags".to_owned())
        );
        assert_eq!(
            read("[defaults]\nturret = 5").unwrap_err(),
            DefaultsError::NotFields("turret".to_owned())
        );
        assert_eq!(
            read("defaults = 5").unwrap_err(),
            DefaultsError::NotSections
        );
    }
}
