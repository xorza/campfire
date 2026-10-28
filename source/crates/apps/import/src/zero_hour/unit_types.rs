use std::collections::BTreeMap;

use campfire_actions::TypeSlotsData;
use campfire_airfields::AirfieldData;
use campfire_codec::{Toml, TomlTable, TomlValue};
use campfire_combat::BarrelsData;
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_math::Num;
use campfire_orders::{AcquireData, RetaliateData};
use campfire_package::UnitsFile;
use campfire_production::{DropOffData, NodeData, ProductionData, SellData, SpawnsData};
use campfire_progression::{ExperienceData, TypeTracksData};
use campfire_stats::UpgradesData;

use crate::zero_hour::area_write::AreaWrite;
use crate::zero_hour::imported_type::{ImportedType, TypeWeapons};
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::mover_import::MoverImport;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::second_life_import::SecondLifeImport;
use crate::zero_hour::type_name::TypeName;
use campfire_areas::AreaData;
use campfire_interaction::{EnterData, RidesData, UsesData};

/// The closed set of the import's unit types, by their names: each the fields its object gives.
/// The stages after it read it and add no type; what they give a type, its actions and its
/// production, they return.
#[derive(Debug)]
pub(crate) struct ImportedTypes {
    types: BTreeMap<TypeName, ImportedType>,
}

/// What the stages give each unit type, by its name: the actions of its slot kinds `weapon`, in its
/// starting weapon set's order, and `command`, in its command set's order, its gather last; its
/// `production` section when it trains; its `sell` when its command set sells it; and its `node`
/// when it is a dock and its `drop_off` when it is a supply center; and its `upgrades`
/// (09-zero-hour.md, Every command set's trains and builds are imported; 09-zero-hour.md, A weapon
/// converts when it hits one target at once; 09-zero-hour.md, A box is money;
/// 04-capabilities/stats.md, A unit type takes modifiers by upgrades).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeAdditions<'a> {
    pub(crate) weapon: &'a BTreeMap<TypeName, Vec<DeclaredName>>,
    /// The weapons of each of its weapon sets' slots, `PRIMARY` first, which its turret carries
    /// by.
    pub(crate) weapon_slots: &'a BTreeMap<TypeName, Vec<Vec<DeclaredName>>>,
    pub(crate) command: &'a BTreeMap<TypeName, Vec<DeclaredName>>,
    pub(crate) production: &'a BTreeMap<TypeName, ProductionData>,
    pub(crate) sells: &'a BTreeMap<TypeName, SellData>,
    pub(crate) airfields: &'a BTreeMap<TypeName, AirfieldData>,
    pub(crate) nodes: &'a BTreeMap<TypeName, NodeData>,
    pub(crate) drop_offs: &'a BTreeMap<TypeName, DropOffData>,
    pub(crate) spawns: &'a BTreeMap<TypeName, SpawnsData>,
    pub(crate) upgrades: &'a BTreeMap<TypeName, UpgradesData>,
    pub(crate) barrels: &'a BTreeMap<TypeName, BarrelsData>,
    /// The bones each type's turret reads of its model.
    pub(crate) bones: &'a BTreeMap<TypeName, ModelBones>,
    /// Its track of veterancy and what its death gives (09-zero-hour.md, Veterancy is progression's
    /// data).
    pub(crate) tracks: &'a BTreeMap<TypeName, TypeTracksData>,
    pub(crate) experience: &'a BTreeMap<TypeName, ExperienceData>,
    /// The area of each marker a power's cast lands (09-zero-hour.md, The import converts a
    /// marker's burst).
    pub(crate) areas: &'a BTreeMap<TypeName, AreaWrite>,
    /// The `enter` of each container, and the `rides` of each rider, which takes a `uses` of the
    /// slot kind of its entry (04-capabilities/interaction.md, A container says who rides).
    pub(crate) enters: &'a BTreeMap<TypeName, EnterData>,
    pub(crate) rides: &'a BTreeMap<TypeName, RidesData>,
    /// The modifiers of the movers' hurts, which a hurt mover's `hurt` names
    /// (04-capabilities/stats.md, A hurt type may hold a modifier).
    pub(crate) movers: &'a MoverImport,
    /// The modifiers of the second lives, which a type's `second_life` names
    /// (04-capabilities/combat.md, A type may live twice).
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
    /// fields its object gives, the slots of the kinds `names` gives, the production and the
    /// supply `additions` give it.
    pub(crate) fn units_file(
        &self,
        additions: TypeAdditions<'_>,
        names: &RulesNames,
    ) -> WrittenUnits {
        let units: BTreeMap<DeclaredName, TomlTable> = self
            .types
            .iter()
            .map(|(name, imported)| {
                let mut writer = SectionWriter::default();
                let weapons = TypeWeapons {
                    all: additions.weapon.get(name).map_or(&[][..], Vec::as_slice),
                    by_slot: additions
                        .weapon_slots
                        .get(name)
                        .map_or(&[][..], Vec::as_slice),
                };
                imported.write(&mut writer, additions.bones.get(name), weapons);
                let hurt = imported
                    .mover()
                    .and_then(|mover| additions.movers.hurt(mover));
                if let Some(hurt) = hurt {
                    writer.section(&hurt);
                }
                if let Some(raise) = imported.second_life() {
                    writer.section(&additions.second_lives.data(raise));
                }
                let slots: BTreeMap<DeclaredName, Vec<DeclaredName>> = [
                    (&names.weapon_slot, additions.weapon),
                    (&names.command_slot, additions.command),
                ]
                .into_iter()
                .filter_map(|(slot, actions)| {
                    let actions = actions.get(name).filter(|actions| !actions.is_empty());
                    Some((slot.clone(), actions?.clone()))
                })
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
                if let Some(production) = additions.production.get(name) {
                    writer.section(production);
                }
                if let Some(sell) = additions.sells.get(name) {
                    writer.section(sell);
                }
                if let Some(airfield) = additions.airfields.get(name) {
                    writer.section(airfield);
                }
                if let Some(node) = additions.nodes.get(name) {
                    writer.section(node);
                }
                if let Some(drop_off) = additions.drop_offs.get(name) {
                    writer.section(drop_off);
                }
                if let Some(spawns) = additions.spawns.get(name) {
                    writer.section(spawns);
                }
                if let Some(upgrades) = additions.upgrades.get(name) {
                    writer.section(upgrades);
                }
                if let Some(barrels) = additions.barrels.get(name) {
                    writer.section(barrels);
                }
                if let Some(tracks) = additions.tracks.get(name) {
                    writer.section(tracks);
                }
                if let Some(experience) = additions.experience.get(name) {
                    writer.section(experience);
                }
                if let Some(area) = additions.areas.get(name) {
                    writer.value::<AreaData>(TomlValue::of(area));
                }
                if let Some(enter) = additions.enters.get(name) {
                    writer.section(enter);
                }
                if let Some(rides) = additions.rides.get(name) {
                    writer.section(rides);
                    writer.section(&UsesData {
                        kind: names.use_slot.clone(),
                    });
                }
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
}

#[cfg(test)]
pub(crate) mod reach_in {
    use std::collections::BTreeMap;

    use crate::zero_hour::mover_import::MoverImport;
    use crate::zero_hour::rules_names::RulesNames;
    use crate::zero_hour::second_life_import::SecondLifeImport;
    use crate::zero_hour::unit_types::{ImportedTypes, TypeAdditions, WrittenUnits};

    impl ImportedTypes {
        /// Its `data/units.toml` of no stage's additions.
        pub(crate) fn bare_file(&self) -> WrittenUnits {
            let additions = TypeAdditions {
                weapon: &BTreeMap::new(),
                weapon_slots: &BTreeMap::new(),
                command: &BTreeMap::new(),
                production: &BTreeMap::new(),
                sells: &BTreeMap::new(),
                airfields: &BTreeMap::new(),
                nodes: &BTreeMap::new(),
                drop_offs: &BTreeMap::new(),
                spawns: &BTreeMap::new(),
                upgrades: &BTreeMap::new(),
                barrels: &BTreeMap::new(),
                bones: &BTreeMap::new(),
                tracks: &BTreeMap::new(),
                experience: &BTreeMap::new(),
                areas: &BTreeMap::new(),
                enters: &BTreeMap::new(),
                rides: &BTreeMap::new(),
                movers: &MoverImport::default(),
                second_lives: &SecondLifeImport::default(),
            };
            self.units_file(additions, &RulesNames::default())
        }
    }
}
