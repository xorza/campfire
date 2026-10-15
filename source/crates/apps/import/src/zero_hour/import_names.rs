use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::num::NonZeroU8;

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
    produced: BTreeMap<ImportName, TypeActions>,
}

/// What a unit type takes of combat and production (design 18, D3; design 19, D6): the actions
/// of its slot kinds `weapon`, in its starting weapon set's order, and `command`, in its command
/// set's order, and its `production` section when it trains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TypeActions {
    weapon: Vec<DeclaredName>,
    command: Vec<DeclaredName>,
    production: Option<ProductionSection>,
}

/// A unit type's `production` section as the import writes it: its queue's length, and its exit
/// and default rally points, `[x, z]` in its own frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct ProductionSection {
    pub(crate) queue: NonZeroU8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit: Option<[Num; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rally: Option<[Num; 2]>,
}

/// A unit type's section of `data/units.toml`: the fields its object gives, its slots and its
/// production.
#[derive(Debug, Serialize)]
struct TypeToml<'a> {
    #[serde(flatten)]
    imported: &'a ImportedType,
    #[serde(skip_serializing_if = "Option::is_none")]
    slots: Option<TypeSlots<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production: Option<ProductionSection>,
}

/// A unit type's `slots`: the actions of its slot kinds `weapon` and `command`.
#[derive(Debug, Serialize)]
struct TypeSlots<'a> {
    #[serde(skip_serializing_if = "<[DeclaredName]>::is_empty")]
    weapon: &'a [DeclaredName],
    #[serde(skip_serializing_if = "<[DeclaredName]>::is_empty")]
    command: &'a [DeclaredName],
}

/// An object's unit type, by its name, and the fields its object gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedType {
    pub(crate) name: DeclaredName,
    pub(crate) imported: ImportedType,
}

/// `data/units.toml`: `[units.<name>]` of each type, the fields its object gives, and its slots
/// and production.
#[derive(Debug, Serialize)]
struct UnitsToml<'a> {
    units: BTreeMap<&'a str, TypeToml<'a>>,
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

    /// The unit type of `object`, when `unit_type` made one.
    pub(crate) fn typed(&self, object: &IniObject) -> Option<&ImportedType> {
        self.placed.get(&object.import_name())
    }

    /// Gives the unit type of `object`, which `unit_type` made, `action` in its slot kind
    /// `weapon`, after those it holds; one it holds already it holds once.
    pub(crate) fn give_weapon(&mut self, object: &IniObject, action: DeclaredName) {
        let entry = self.produced.entry(object.import_name()).or_default();
        if !entry.weapon.contains(&action) {
            entry.weapon.push(action);
        }
    }

    /// Gives the unit type of `object`, which `unit_type` made, `action` in its slot kind
    /// `command`, after those it holds.
    pub(crate) fn give_command(&mut self, object: &IniObject, action: DeclaredName) {
        let entry = self.produced.entry(object.import_name()).or_default();
        if !entry.command.contains(&action) {
            entry.command.push(action);
        }
    }

    /// Gives the unit type of `object`, which `unit_type` made, its `production` section.
    pub(crate) fn give_production(&mut self, object: &IniObject, section: ProductionSection) {
        self.produced
            .entry(object.import_name())
            .or_default()
            .production = Some(section);
    }

    /// The package's `data/units.toml`: each unit type the maps place, the start makes or the
    /// command sets make, in the order of their names, of the fields its object gives, its
    /// slots and its production.
    pub(crate) fn units_file(&self) -> UnitsFile {
        let units: BTreeMap<&str, TypeToml<'_>> = self
            .placed
            .iter()
            .map(|(name, imported)| {
                let produced = self.produced.get(name);
                let section = TypeToml {
                    imported,
                    slots: produced
                        .filter(|produced| {
                            !produced.weapon.is_empty() || !produced.command.is_empty()
                        })
                        .map(|produced| TypeSlots {
                            weapon: &produced.weapon,
                            command: &produced.command,
                        }),
                    production: produced.and_then(|produced| produced.production),
                };
                (name.as_str(), section)
            })
            .collect();
        let max_speed = units
            .values()
            .filter_map(|unit| unit.imported.move_speed())
            .max();
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
