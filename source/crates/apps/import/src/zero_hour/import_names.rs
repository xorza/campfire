use std::collections::{BTreeMap, BTreeSet};

use campfire_codec::Toml;
use campfire_values::DeclaredName;
use serde::Serialize;
use toml::Table;

use crate::zero_hour::error::NameClash;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::object_ini::IniObject;

/// The name each object of the original takes in the package, for its look and for its unit
/// type, with the object that took it, so a look and a unit type of one name are of one object:
/// the second object to give a name is refused. It keeps the unit types the maps place.
#[derive(Debug, Default)]
pub(crate) struct ImportNames {
    taken: BTreeMap<ImportName, String>,
    placed: BTreeSet<ImportName>,
}

/// `data/units.toml` of types with no field yet: `[units.<name>]`, each empty.
#[derive(Debug, Serialize)]
struct UnitsToml<'a> {
    units: BTreeMap<&'a str, Table>,
}

impl ImportNames {
    /// The name `object` takes; an error when another object took it.
    pub(crate) fn take(&mut self, object: &IniObject) -> Result<ImportName, NameClash> {
        let name = object.import_name();
        let taker = self
            .taken
            .entry(name.clone())
            .or_insert_with(|| object.name.clone());
        if *taker != object.name {
            return Err(NameClash {
                first: taker.clone(),
                second: object.name.clone(),
            });
        }
        Ok(name)
    }

    /// The unit type of `object`, which a map places, its name taken as `take` takes it.
    pub(crate) fn unit_type(&mut self, object: &IniObject) -> Result<DeclaredName, NameClash> {
        let name = self.take(object)?;
        let declared = name.declared();
        self.placed.insert(name);
        Ok(declared)
    }

    /// The package's `data/units.toml`: each unit type the maps place, in the order of their
    /// names.
    pub(crate) fn units_toml(&self) -> String {
        let units = self
            .placed
            .iter()
            .map(|name| (name.as_str(), Table::new()))
            .collect();
        Toml::write(&UnitsToml { units })
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde::de::IgnoredAny;

    use super::*;
    use crate::zero_hour::object_ini::ObjectIni;

    #[test]
    fn an_objects_name_is_its_own_unless_another_took_it() {
        // Each type's section reads as the engine's unit type where the import's package loads.
        #[derive(Debug, Deserialize)]
        struct Units {
            #[expect(
                clippy::zero_sized_map_values,
                reason = "the test reads each section's name alone"
            )]
            units: BTreeMap<DeclaredName, IgnoredAny>,
        }
        let mut ini = ObjectIni::default();
        let objects =
            "Object Rocks1\nEnd\nObject AmericaTank\nEnd\nObject ROCKS1\nEnd\nObject Oak\nEnd\n";
        ini.read(objects.as_bytes()).unwrap();
        let object = |name: &str| ini.object(name.as_bytes()).unwrap();
        let mut names = ImportNames::default();
        // A look's name, then the same object placed: one name. Another object placed.
        assert_eq!(names.take(object("Rocks1")).unwrap().as_str(), "rocks1");
        assert_eq!(
            names.unit_type(object("Rocks1")).unwrap().as_str(),
            "rocks1"
        );
        assert_eq!(
            names.unit_type(object("AmericaTank")).unwrap().as_str(),
            "americatank"
        );
        // Another object of the same name is refused, as a look or a unit type alike.
        let clash = NameClash {
            first: "Rocks1".to_owned(),
            second: "ROCKS1".to_owned(),
        };
        assert_eq!(names.take(object("ROCKS1")), Err(clash.clone()));
        assert_eq!(names.unit_type(object("ROCKS1")), Err(clash));
        // A look alone is no unit type: the file lists the placed types.
        names.take(object("Oak")).unwrap();
        let toml = names.units_toml();
        assert_eq!(toml, "[units.americatank]\n\n[units.rocks1]\n");
        let units: Units = Toml::parse(&toml).unwrap();
        let read: Vec<&str> = units.units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(read, ["americatank", "rocks1"]);
    }
}
