use std::collections::BTreeMap;

use campfire_capabilities::DeclaredName;
use campfire_common::Toml;
use serde::Serialize;
use toml::Table;

use crate::zero_hour::error::MapError;
use crate::zero_hour::import_name::ImportName;

/// The unit types of every map imported so far: each template an object names, by the name its
/// unit type takes, refused when two templates take one name.
#[derive(Debug, Default)]
pub(crate) struct UnitTypes {
    templates: BTreeMap<ImportName, Vec<u8>>,
}

/// `data/units.toml` of types with no field yet: `[units.<name>]`, each empty.
#[derive(Debug, Serialize)]
struct UnitsToml<'a> {
    units: BTreeMap<&'a str, Table>,
}

impl UnitTypes {
    /// The unit type of `template`.
    pub(crate) fn name(&mut self, template: &[u8]) -> Result<DeclaredName, MapError> {
        let name = ImportName::of(template);
        let declared = name.declared();
        let known = self
            .templates
            .entry(name)
            .or_insert_with(|| template.to_vec());
        if *known != template {
            return Err(MapError::UnitTypeClash {
                first: String::from_utf8_lossy(known).into_owned(),
                second: String::from_utf8_lossy(template).into_owned(),
            });
        }
        Ok(declared)
    }

    /// The package's `data/units.toml`, in the order of the types' names.
    pub(crate) fn toml(&self) -> String {
        let units = self
            .templates
            .keys()
            .map(|name| (name.as_str(), Table::new()))
            .collect();
        Toml::write(&UnitsToml { units }).expect("units encode as TOML")
    }
}

#[cfg(test)]
mod tests {
    use campfire_capabilities::UnitTypeFile;
    use serde::Deserialize;

    use super::*;

    #[test]
    fn templates_become_unit_types_unless_two_take_one_name() {
        #[derive(Debug, Deserialize)]
        struct Units {
            units: BTreeMap<DeclaredName, UnitTypeFile>,
        }
        let mut types = UnitTypes::default();
        assert_eq!(types.name(b"Rocks1").unwrap().as_str(), "rocks1");
        assert_eq!(types.name(b"AmericaTank").unwrap().as_str(), "americatank");
        assert_eq!(types.name(b"Rocks1").unwrap().as_str(), "rocks1");
        assert_eq!(
            types.name(b"ROCKS1"),
            Err(MapError::UnitTypeClash {
                first: "Rocks1".to_owned(),
                second: "ROCKS1".to_owned()
            })
        );
        let toml = types.toml();
        assert_eq!(toml, "[units.americatank]\n\n[units.rocks1]\n");

        let units: Units = Toml::parse(&toml).unwrap();
        assert_eq!(
            units
                .units
                .keys()
                .map(DeclaredName::as_str)
                .collect::<Vec<_>>(),
            ["americatank", "rocks1"]
        );
    }
}
