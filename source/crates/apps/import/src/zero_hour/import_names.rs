use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use campfire_codec::Toml;
use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::error::{NameClash, TypeError};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::locomotor_ini::LocomotorIni;
use crate::zero_hour::object_ini::IniObject;

/// The name each object of the original takes in the package, for its look and for its unit
/// type, with the object that took it, so a look and a unit type of one name are of one object:
/// the second object to give a name is refused. It keeps the unit types the maps place, each
/// with the fields its object gives.
#[derive(Debug, Default)]
pub(crate) struct ImportNames {
    taken: BTreeMap<ImportName, String>,
    placed: BTreeMap<ImportName, ImportedType>,
}

/// An object's unit type, by its name, and the fields its object gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedType {
    pub(crate) name: DeclaredName,
    pub(crate) imported: ImportedType,
}

/// `data/units.toml`: `[units.<name>]` of each type, the fields its object gives.
#[derive(Debug, Serialize)]
struct UnitsToml<'a> {
    units: BTreeMap<&'a str, ImportedType>,
}

/// The package's `data/units.toml`, and the largest speed of its types, when one drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitsFile {
    pub(crate) text: String,
    pub(crate) max_speed: Option<Num>,
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

    /// The unit type of `object`, which a map places, its name taken as `take` takes it, and its
    /// locomotors those of `locomotors`.
    pub(crate) fn unit_type(
        &mut self,
        object: &IniObject,
        locomotors: &LocomotorIni,
    ) -> Result<NamedType, TypeError> {
        let name = self.take(object)?;
        let declared = name.declared();
        let imported = match self.placed.entry(name) {
            Entry::Occupied(placed) => *placed.get(),
            Entry::Vacant(new) => *new.insert(ImportedType::of(object, locomotors)?),
        };
        Ok(NamedType {
            name: declared,
            imported,
        })
    }

    /// The package's `data/units.toml`: each unit type the maps place, in the order of their
    /// names, of the fields its object gives.
    pub(crate) fn units_file(&self) -> UnitsFile {
        let units: BTreeMap<&str, ImportedType> = self
            .placed
            .iter()
            .map(|(name, &imported)| (name.as_str(), imported))
            .collect();
        let max_speed = units.values().filter_map(ImportedType::move_speed).max();
        UnitsFile {
            text: Toml::write(&UnitsToml { units }),
            max_speed,
        }
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
        let locomotors = LocomotorIni::default();
        let mut names = ImportNames::default();
        // A look's name, then the same object placed: one name. Another object placed.
        assert_eq!(names.take(object("Rocks1")).unwrap().as_str(), "rocks1");
        for (object_name, name) in [("Rocks1", "rocks1"), ("AmericaTank", "americatank")] {
            let placed = names.unit_type(object(object_name), &locomotors).unwrap();
            assert_eq!(placed.name.as_str(), name);
        }
        // Another object of the same name is refused, as a look or a unit type alike.
        let clash = NameClash {
            first: "Rocks1".to_owned(),
            second: "ROCKS1".to_owned(),
        };
        assert_eq!(names.take(object("ROCKS1")), Err(clash.clone()));
        let refused = names.unit_type(object("ROCKS1"), &locomotors);
        assert_eq!(refused, Err(TypeError::NameClash(clash)));
        // A look alone is no unit type: the file lists the placed types, which neither drive nor
        // block, so no speed bounds them.
        names.take(object("Oak")).unwrap();
        let file = names.units_file();
        assert_eq!(file.max_speed, None);
        let toml = file.text;
        assert_eq!(toml, "[units.americatank]\n\n[units.rocks1]\n");
        let units: Units = Toml::parse(&toml).unwrap();
        let read: Vec<&str> = units.units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(read, ["americatank", "rocks1"]);
    }
}
