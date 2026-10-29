use std::collections::BTreeMap;

use campfire_actions::TypeSlotsData;
use campfire_areas::AreaData;
use campfire_codec::{Toml, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_interaction::UsesData;
use campfire_math::Num;
use campfire_orders::{AcquireData, RetaliateData};
use campfire_package::UnitsFile;

use crate::zero_hour::additions::{Additions, TypeAdditions};
use crate::zero_hour::imported_type::{ImportedType, TypeWeapons};
use crate::zero_hour::mover_import::MoverImport;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::second_life_import::SecondLifeImport;
use crate::zero_hour::type_name::TypeName;

/// The closed set of the import's unit types, by their names: each the fields its object gives.
/// The stages after it read it and add no type; what the stages give a type, its actions and its
/// production among them, its additions hold.
#[derive(Debug)]
pub(crate) struct ImportedTypes {
    types: BTreeMap<TypeName, ImportedType>,
}

/// The modifiers the units' file names of the types' movers' hurts and second lives
/// (04-capabilities/stats.md, A hurt type may hold a modifier; 04-capabilities/combat.md, A type
/// may live twice).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeModifiers<'a> {
    pub(crate) movers: &'a MoverImport,
    pub(crate) second_lives: &'a SecondLifeImport,
}

/// The package's `data/units.toml`, and the largest speed of its types, when one drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WrittenUnits {
    pub(crate) text: String,
    pub(crate) max_speed: Option<Num>,
}

impl ImportedTypes {
    pub(crate) const fn new(types: BTreeMap<TypeName, ImportedType>) -> Self {
        Self { types }
    }

    /// Whether it holds the type of `name`.
    pub(crate) fn holds(&self, name: &DeclaredName) -> bool {
        self.types.contains_key(name)
    }

    /// The modifiers of its types' second lives, of `names`' life stat and tag.
    pub(crate) fn second_lives(&self, names: &RulesNames) -> SecondLifeImport {
        SecondLifeImport::of(
            self.types.values().filter_map(ImportedType::second_life),
            names,
        )
    }

    /// The modifiers of its movers' hurts.
    pub(crate) fn movers(&self) -> MoverImport {
        MoverImport::of(self.types.values().filter_map(ImportedType::mover))
    }

    /// The package's `data/units.toml`: each unit type in the order of their names, of the
    /// fields its object gives, what `additions` gives it, its slots of the kinds `names` gives,
    /// and the `modifiers` of its hurt and its second life.
    pub(crate) fn units_file(
        &self,
        additions: &Additions,
        modifiers: TypeModifiers<'_>,
        names: &RulesNames,
    ) -> WrittenUnits {
        let none = TypeAdditions::default();
        let units: BTreeMap<DeclaredName, TomlTable> = self
            .types
            .iter()
            .map(|(name, imported)| {
                let added = additions.get(name).unwrap_or(&none);
                let mut writer = SectionWriter::default();
                let weapons = TypeWeapons {
                    all: &added.weapons,
                    by_slot: &added.weapon_slots,
                };
                imported.write(&mut writer, added.bones.as_ref(), weapons);
                let hurt = imported
                    .mover()
                    .and_then(|mover| modifiers.movers.hurt(mover));
                if let Some(hurt) = hurt {
                    writer.section(&hurt);
                }
                if let Some(raise) = imported.second_life() {
                    writer.section(&modifiers.second_lives.data(raise));
                }
                let slots: BTreeMap<DeclaredName, Vec<DeclaredName>> = [
                    (&names.weapon_slot, &added.weapons),
                    (&names.command_slot, &added.commands),
                ]
                .into_iter()
                .filter(|(_, actions)| !actions.is_empty())
                .map(|(slot, actions)| (slot.clone(), actions.clone()))
                .collect();
                // A scan and an answer take a weapon to attack what they find.
                let armed = slots.contains_key(&names.weapon_slot);
                if !slots.is_empty() {
                    writer.section(&TypeSlotsData(slots));
                }
                if let Some(acquire) = imported.acquire().filter(|_| armed) {
                    writer.patch::<AcquireData>(acquire);
                }
                if let Some(retaliate) = imported.retaliate().filter(|_| armed) {
                    writer.patch::<RetaliateData>(retaliate);
                }
                Self::write_additions(&mut writer, added, names);
                (name.declared().clone(), writer.into_table())
            })
            .collect();
        let max_speed = self
            .types
            .values()
            .filter_map(ImportedType::move_speed)
            .max();
        WrittenUnits {
            text: Toml::write(&UnitsFile::of(&units)),
            max_speed,
        }
    }

    /// Writes the sections of `added` into `writer`, a rider's `uses` of the slot kind `names`
    /// gives its entry beside its `rides`.
    fn write_additions(writer: &mut SectionWriter, added: &TypeAdditions, names: &RulesNames) {
        let TypeAdditions {
            weapons: _,
            weapon_slots: _,
            commands: _,
            bones: _,
            fire_points: _,
            carried: _,
            production,
            sell,
            airfield,
            node,
            drop_off,
            spawns,
            upgrades,
            barrels,
            tracks,
            experience,
            area,
            enter,
            rides,
        } = added;
        if let Some(production) = production {
            writer.section(production);
        }
        if let Some(sell) = sell {
            writer.section(sell);
        }
        if let Some(airfield) = airfield {
            writer.section(airfield);
        }
        if let Some(node) = node {
            writer.section(node);
        }
        if let Some(drop_off) = drop_off {
            writer.section(drop_off);
        }
        if let Some(spawns) = spawns {
            writer.section(spawns);
        }
        if let Some(upgrades) = upgrades {
            writer.section(upgrades);
        }
        if let Some(barrels) = barrels {
            writer.section(barrels);
        }
        if let Some(tracks) = tracks {
            writer.section(tracks);
        }
        if let Some(experience) = experience {
            writer.section(experience);
        }
        if let Some(area) = area {
            writer.value::<AreaData>(TomlValue::of(area));
        }
        if let Some(enter) = enter {
            writer.section(enter);
        }
        if let Some(rides) = rides {
            writer.section(rides);
            writer.section(&UsesData {
                kind: names.use_slot.clone(),
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use crate::zero_hour::additions::Additions;
    use crate::zero_hour::mover_import::MoverImport;
    use crate::zero_hour::rules_names::RulesNames;
    use crate::zero_hour::second_life_import::SecondLifeImport;
    use crate::zero_hour::unit_types::{ImportedTypes, TypeModifiers, WrittenUnits};

    impl ImportedTypes {
        /// Its `data/units.toml` of no stage's additions.
        pub(crate) fn bare_file(&self) -> WrittenUnits {
            let modifiers = TypeModifiers {
                movers: &MoverImport::default(),
                second_lives: &SecondLifeImport::default(),
            };
            self.units_file(&Additions::default(), modifiers, &RulesNames::default())
        }
    }
}
