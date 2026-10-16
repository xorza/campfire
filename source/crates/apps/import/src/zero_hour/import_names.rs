use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use campfire_values::DeclaredName;

use crate::zero_hour::error::{NameClash, TypeError};
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::unit_types::{TypedObject, UnitTypes};

/// The name each object of the original takes in the package, for its look and for its unit
/// type, with the object that took it, so a look and a unit type of one name are of one object:
/// the second object to give a name is refused. It keeps the unit types the maps place, the
/// start makes and the command sets make, each with the fields its object gives, until `close`
/// gives their closed set.
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

impl ImportNames {
    /// The name `object` takes; an error when another object took it.
    pub(crate) fn take(&mut self, object: &IniObject) -> Result<ImportName, NameClash> {
        let name = ImportName::object(object);
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

    /// The unit type of `object`, which a map places, the start makes or a command set makes,
    /// its name taken as `take` takes it, and its locomotors those of `locomotors`.
    pub(crate) fn unit_type(
        &mut self,
        object: &IniObject,
        locomotors: &LocomotorIni,
    ) -> Result<NamedType, TypeError> {
        let name = self.take(object)?;
        let declared = name.declared();
        let imported = match self.placed.entry(name) {
            Entry::Occupied(placed) => placed.get().clone(),
            Entry::Vacant(new) => new.insert(ImportedType::of(object, locomotors)?).clone(),
        };
        Ok(NamedType {
            name: declared,
            imported,
        })
    }

    /// The object names of the unit types so far, in the order of their names.
    pub(crate) fn typed_objects(&self) -> Vec<String> {
        self.placed
            .keys()
            .map(|name| self.taken[name].clone())
            .collect()
    }

    /// The closed set of the unit types it made, which no stage after adds to.
    pub(crate) fn close(self) -> UnitTypes {
        let mut taken = self.taken;
        let objects = self
            .placed
            .into_iter()
            .map(|(name, imported)| {
                let object = taken.remove(&name).expect("a unit type takes its name");
                (name, TypedObject { object, imported })
            })
            .collect();
        UnitTypes::new(objects)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use serde::Deserialize;
    use serde::de::IgnoredAny;

    use super::*;
    use crate::zero_hour::format::ini::object_ini::ObjectIni;

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
        let file = names.close().bare_file();
        assert_eq!(file.max_speed, None);
        let toml = file.text;
        assert_eq!(toml, "[units.americatank]\n\n[units.rocks1]\n");
        let units: Units = Toml::parse(&toml).unwrap();
        let read: Vec<&str> = units.units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(read, ["americatank", "rocks1"]);
    }
}
