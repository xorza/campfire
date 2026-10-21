use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use campfire_common::DeclaredName;

use crate::zero_hour::error::{NameClash, TypeError};
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::unit_types::ImportedTypes;

/// The name each object of the original takes in the package, for its look and for its unit
/// type, with the object that took it, so a look and a unit type of one name are of one object:
/// the second object to give a name is refused. It keeps the unit types each stage makes, each
/// with the fields its object gives, in three phases its types state, each of the stages that
/// may run in it: the looks and the placements of the maps and the start; then production, which
/// starts from every placed type; then the projectiles combat's weapons fire, which are none of
/// the others; `close` gives their closed set.
#[derive(Debug, Default)]
pub(crate) struct ImportNames {
    taken: BTreeMap<ImportName, String>,
    placed: BTreeMap<ImportName, ImportedType>,
}

/// The names as production makes the types its command sets make, every placement done.
#[derive(Debug)]
pub(crate) struct ProductionNames(ImportNames);

/// The names as combat names the projectiles its weapons fire, every other type made.
#[derive(Debug)]
pub(crate) struct ProjectileNames(ImportNames);

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

    /// Ends the placements, so production starts from every placed type.
    pub(crate) const fn into_production(self) -> ProductionNames {
        ProductionNames(self)
    }

    /// The unit type of the projectile `object` fires, `imported`, by `name`, its object's own or
    /// one of its own for a weapon's projectile; an error when another object took the name, or when the
    /// object's own unit type took it.
    fn projectile(
        &mut self,
        name: ImportName,
        object: &IniObject,
        imported: ImportedType,
    ) -> Result<DeclaredName, NameClash> {
        let taker = self
            .taken
            .entry(name.clone())
            .or_insert_with(|| object.name.clone());
        if *taker != object.name || self.placed.contains_key(&name) {
            return Err(NameClash {
                first: taker.clone(),
                second: object.name.clone(),
            });
        }
        let declared = name.declared();
        self.placed.insert(name, imported);
        Ok(declared)
    }

    /// Each unit type so far, by its name, with its object's name and its fields, in the order of
    /// their names.
    fn types(&self) -> impl Iterator<Item = (&ImportName, &str, &ImportedType)> {
        self.placed
            .iter()
            .map(|(name, imported)| (name, self.taken[name].as_str(), imported))
    }

    /// The object names of the unit types so far, in the order of their names.
    fn typed_objects(&self) -> Vec<String> {
        self.placed
            .keys()
            .map(|name| self.taken[name].clone())
            .collect()
    }
}

impl ProductionNames {
    /// The object names of the placed types, and those production made so far, in the order of
    /// their names.
    pub(crate) fn typed_objects(&self) -> Vec<String> {
        self.0.typed_objects()
    }

    /// The unit type of `object`, which a command set makes, as `ImportNames::unit_type` makes
    /// it.
    pub(crate) fn unit_type(
        &mut self,
        object: &IniObject,
        locomotors: &LocomotorIni,
    ) -> Result<NamedType, TypeError> {
        self.0.unit_type(object, locomotors)
    }

    /// Ends production, so combat names its projectiles beside every other type.
    pub(crate) fn into_projectiles(self) -> ProjectileNames {
        ProjectileNames(self.0)
    }
}

impl ProjectileNames {
    /// Each unit type but the projectiles, and the projectiles named so far, as
    /// `ImportNames::types` gives them.
    pub(crate) fn types(&self) -> impl Iterator<Item = (&ImportName, &str, &ImportedType)> {
        self.0.types()
    }

    /// The unit type of a projectile, as `ImportNames::projectile` names it.
    pub(crate) fn projectile(
        &mut self,
        name: ImportName,
        object: &IniObject,
        imported: ImportedType,
    ) -> Result<DeclaredName, NameClash> {
        self.0.projectile(name, object, imported)
    }

    /// The closed set of the unit types the stages made, which no stage after adds to.
    pub(crate) fn close(self) -> ImportedTypes {
        ImportedTypes::new(self.0.placed)
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Toml, TomlTable};

    use super::*;
    use crate::zero_hour::format::ini::object_ini::ObjectIni;

    #[test]
    fn an_objects_name_is_its_own_unless_another_took_it() {
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
        let file = names
            .into_production()
            .into_projectiles()
            .close()
            .bare_file();
        assert_eq!(file.max_speed, None);
        let toml = file.text;
        assert_eq!(toml, "[units.americatank]\n\n[units.rocks1]\n");
        // Each type's section reads by its name as the engine's unit type, where the import's
        // package loads.
        let mut file: TomlTable = Toml::parse(&toml).unwrap();
        let units = file.take::<BTreeMap<DeclaredName, TomlTable>>("units");
        let units = units.unwrap().unwrap();
        let read: Vec<&str> = units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(read, ["americatank", "rocks1"]);
    }
}
