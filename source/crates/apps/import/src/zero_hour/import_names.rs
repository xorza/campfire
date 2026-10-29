use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use crate::zero_hour::error::{NameClash, NameError, TypeError};
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::unit_types::ImportedTypes;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The name each object of the original takes in the package, for its look and for its unit
/// type, with the object that took it, so a look and a unit type of one name are of one object:
/// the second object to give a name is refused. It keeps the unit types each stage makes, each
/// with the fields its object gives, in three phases its types state, each of the stages that
/// may run in it: the looks and the placements of the maps and the start; then production, which
/// starts from every placed type; then the projectiles combat's weapons fire, which are none of
/// the others; `close` gives their closed set. Each type's stages read the object that fields it:
/// its own, or a parent's first build variation
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Debug, Default)]
pub(crate) struct ImportNames<'r> {
    taken: BTreeMap<TypeName, &'r IniObject>,
    types: BTreeMap<TypeName, TypeEntry<'r>>,
}

/// A unit type made so far: the fields its object gives, and the object that fields it.
#[derive(Debug)]
struct TypeEntry<'r> {
    imported: ImportedType,
    object: &'r IniObject,
}

/// The names as production makes the types its command sets make, every placement done, and the
/// objects it typed that the walk of command sets has yet to visit.
#[derive(Debug)]
pub(crate) struct ProductionNames<'r> {
    names: ImportNames<'r>,
    fresh: Vec<NamedObject<'r>>,
}

/// The object that fields a unit type, and the type's name.
#[derive(Debug, Clone)]
pub(crate) struct NamedObject<'r> {
    pub(crate) name: TypeName,
    pub(crate) object: &'r IniObject,
}

/// The names as combat names the projectiles its weapons fire, every other type made.
#[derive(Debug)]
pub(crate) struct ProjectileNames<'r>(ImportNames<'r>);

/// An object's unit type, by its name, the fields its object gives, and the object that fields
/// it, which every stage after reads for it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NamedType<'n, 'r> {
    pub(crate) name: &'n TypeName,
    pub(crate) imported: &'n ImportedType,
    pub(crate) object: &'r IniObject,
}

impl<'r> ImportNames<'r> {
    /// The name `object` takes; an error when another object took it, or when it passes the
    /// bytes a declared name holds.
    pub(crate) fn take(&mut self, object: &'r IniObject) -> Result<TypeName, NameError> {
        let name = TypeName::of(object)?;
        self.claim(&name, object)?;
        Ok(name)
    }

    /// The unit type of `object`, which a map places, the start makes or a command set makes,
    /// its name taken as `take` takes it, and its fields those `rules` give it.
    pub(crate) fn unit_type(
        &mut self,
        object: &'r IniObject,
        rules: &'r ZeroHourRules,
    ) -> Result<NamedType<'_, 'r>, TypeError> {
        let name = self.take(object)?;
        if !self.types.contains_key(&name) {
            let entry = TypeEntry {
                imported: ImportedType::of(object, rules)?,
                object: rules.variations.fielded(&rules.objects, object),
            };
            self.types.insert(name.clone(), entry);
        }
        let (name, entry) = self.types.get_key_value(&name).expect("the type is made");
        Ok(NamedType {
            name,
            imported: &entry.imported,
            object: entry.object,
        })
    }

    /// Ends the placements, so production starts from every placed type.
    pub(crate) const fn into_production(self) -> ProductionNames<'r> {
        ProductionNames {
            names: self,
            fresh: Vec::new(),
        }
    }

    /// `name` for `object`; an error when another object took it.
    fn claim(&mut self, name: &TypeName, object: &'r IniObject) -> Result<(), NameClash> {
        let taker = *self.taken.entry(name.clone()).or_insert(object);
        if taker.id == object.id {
            Ok(())
        } else {
            Err(NameClash {
                first: taker.name.clone(),
                second: object.name.clone(),
            })
        }
    }

    /// The unit type of the projectile `object` fires, `imported`, by `name`, its object's own or
    /// one of its own for a weapon's projectile; an error when another object took the name, or
    /// when the object's own unit type took it.
    fn projectile(
        &mut self,
        name: TypeName,
        object: &'r IniObject,
        imported: ImportedType,
    ) -> Result<TypeName, NameError> {
        self.claim(&name, object)?;
        match self.types.entry(name.clone()) {
            Entry::Occupied(_) => Err(NameError::ProjectileOfType {
                object: object.name.clone(),
                name: name.to_string(),
            }),
            Entry::Vacant(new) => {
                new.insert(TypeEntry { imported, object });
                Ok(name)
            }
        }
    }

    /// Each unit type so far, in the order of their names.
    fn types(&self) -> impl Iterator<Item = NamedType<'_, 'r>> {
        self.types.iter().map(|(name, entry)| NamedType {
            name,
            object: entry.object,
            imported: &entry.imported,
        })
    }

    /// The objects of the unit types so far, in the order of their names.
    fn typed_objects(&self) -> Vec<NamedObject<'r>> {
        let named = |(name, entry): (&TypeName, &TypeEntry<'r>)| NamedObject {
            name: name.clone(),
            object: entry.object,
        };
        self.types.iter().map(named).collect()
    }
}

impl<'r> ProductionNames<'r> {
    /// The objects of the placed types, and those production made so far, in the order of their
    /// names.
    pub(crate) fn typed_objects(&self) -> Vec<NamedObject<'r>> {
        self.names.typed_objects()
    }

    /// The unit type of `object`, which a command set makes or its prerequisites name, as
    /// `ImportNames::unit_type` makes it; an object typed for the first time
    /// joins the ones `take_fresh` gives, by the object that fields it.
    pub(crate) fn unit_type(
        &mut self,
        object: &'r IniObject,
        rules: &'r ZeroHourRules,
    ) -> Result<NamedType<'_, 'r>, TypeError> {
        let typed = TypeName::of(object).is_ok_and(|name| self.names.types.contains_key(&name));
        let named = self.names.unit_type(object, rules)?;
        if !typed {
            self.fresh.push(NamedObject {
                name: named.name.clone(),
                object: named.object,
            });
        }
        Ok(named)
    }

    /// The fields of the unit type `name`, which production typed or a map placed.
    pub(crate) fn imported(&self, name: &TypeName) -> &ImportedType {
        &self.names.types[name].imported
    }

    /// The objects typed since the last take, in the order they were.
    pub(crate) fn take_fresh(&mut self) -> std::vec::Drain<'_, NamedObject<'r>> {
        self.fresh.drain(..)
    }

    /// Ends production, so combat names its projectiles beside every other type.
    pub(crate) fn into_projectiles(self) -> ProjectileNames<'r> {
        ProjectileNames(self.names)
    }
}

impl<'r> ProjectileNames<'r> {
    /// Each unit type but the projectiles, and the projectiles named so far, as
    /// `ImportNames::types` gives them.
    pub(crate) fn types(&self) -> impl Iterator<Item = NamedType<'_, 'r>> {
        self.0.types()
    }

    /// The unit type of a projectile, as `ImportNames::projectile` names it.
    pub(crate) fn projectile(
        &mut self,
        name: TypeName,
        object: &'r IniObject,
        imported: ImportedType,
    ) -> Result<TypeName, NameError> {
        self.0.projectile(name, object, imported)
    }

    /// The closed set of the unit types the stages made, which no stage after adds to.
    pub(crate) fn close(self) -> ImportedTypes {
        let types = self.0.types.into_iter();
        ImportedTypes::new(types.map(|(name, entry)| (name, entry.imported)).collect())
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Toml, TomlTable};
    use campfire_common::DeclaredName;

    use super::*;
    use crate::zero_hour::format::ini::object_ini::ObjectFiles;

    #[test]
    fn an_objects_name_is_its_own_unless_another_took_it() {
        let mut ini = ObjectFiles::default();
        let objects =
            "Object Rocks1\nEnd\nObject AmericaTank\nEnd\nObject ROCKS1\nEnd\nObject Oak\nEnd\n";
        ini.read(objects.as_bytes()).unwrap();
        let ini = ini.finish();
        let object = |name: &str| ini.object(name.as_bytes()).unwrap();
        let rules = ZeroHourRules::default();
        let mut names = ImportNames::default();
        // A look's name, then the same object placed: one name. Another object placed.
        assert_eq!(names.take(object("Rocks1")).unwrap().as_str(), "rocks1");
        for (object_name, name) in [("Rocks1", "rocks1"), ("AmericaTank", "americatank")] {
            let placed = names.unit_type(object(object_name), &rules).unwrap();
            assert_eq!(placed.name.as_str(), name);
        }
        // Another object of the same name is refused, as a look or a unit type alike.
        let clash = NameClash {
            first: "Rocks1".to_owned(),
            second: "ROCKS1".to_owned(),
        };
        assert_eq!(names.take(object("ROCKS1")), Err(clash.clone().into()));
        let refused = names.unit_type(object("ROCKS1"), &rules).unwrap_err();
        assert_eq!(refused, TypeError::Name(clash.into()));
        // A look alone is no unit type: the file lists the placed types, which neither drive nor
        // block, so no speed bounds them.
        names.take(object("Oak")).unwrap();
        let tank = names.unit_type(object("AmericaTank"), &rules).unwrap();
        let (name, object, imported) = (tank.name.clone(), tank.object, tank.imported.clone());
        let mut projectiles = names.into_production().into_projectiles();
        // A projectile of the tank's that gives the tank's own name is refused, naming both.
        let own = projectiles.projectile(name, object, imported);
        let refused = NameError::ProjectileOfType {
            object: "AmericaTank".to_owned(),
            name: "americatank".to_owned(),
        };
        assert_eq!(own, Err(refused));
        let file = projectiles.close().bare_file();
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
