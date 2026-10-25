use std::collections::BTreeMap;

use campfire_actions::TypeSlotsData;
use campfire_codec::{Toml, TomlTable};
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_math::Num;
use campfire_orders::AcquireData;
use campfire_package::UnitsFile;
use campfire_production::{DropOffData, NodeData, ProductionData, SpawnsData};
use campfire_stats::UpgradesData;

use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::type_name::TypeName;

/// The closed set of the import's unit types, by their names: each the fields its object gives.
/// The stages after it read it and add no type; what they give a type, its actions and its
/// production, they return.
#[derive(Debug)]
pub(crate) struct ImportedTypes {
    types: BTreeMap<TypeName, ImportedType>,
}

/// What the stages give each unit type, by its name: the actions of its slot kinds `weapon`, in its
/// starting weapon set's order, and `command`, in its command set's order, its gather last; its
/// `production` section when it trains; and its `node` when it is a dock and its `drop_off` when
/// it is a supply center; and its `upgrades` (09-zero-hour.md, Every command set's trains and
/// builds are imported; 09-zero-hour.md, A weapon converts when it hits one target at once;
/// 09-zero-hour.md, A box is money; upgrades.md, A unit type takes modifiers by upgrades).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeAdditions<'a> {
    pub(crate) weapon: &'a BTreeMap<TypeName, Vec<DeclaredName>>,
    pub(crate) command: &'a BTreeMap<TypeName, Vec<DeclaredName>>,
    pub(crate) production: &'a BTreeMap<TypeName, ProductionData>,
    pub(crate) nodes: &'a BTreeMap<TypeName, NodeData>,
    pub(crate) drop_offs: &'a BTreeMap<TypeName, DropOffData>,
    pub(crate) spawns: &'a BTreeMap<TypeName, SpawnsData>,
    pub(crate) upgrades: &'a BTreeMap<TypeName, UpgradesData>,
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
                imported.write(&mut writer);
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
                // A scan takes a weapon to attack what it finds.
                let armed = slots.contains_key(&names.weapon_slot);
                if !slots.is_empty() {
                    writer.section(&TypeSlotsData(slots));
                }
                if let Some(acquire) = imported.acquire().filter(|_| armed) {
                    writer.patch::<AcquireData>(acquire);
                }
                if let Some(production) = additions.production.get(name) {
                    writer.section(production);
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

    use crate::zero_hour::rules_names::RulesNames;
    use crate::zero_hour::unit_types::{ImportedTypes, TypeAdditions, WrittenUnits};

    impl ImportedTypes {
        /// Its `data/units.toml` of no stage's additions.
        pub(crate) fn bare_file(&self) -> WrittenUnits {
            let additions = TypeAdditions {
                weapon: &BTreeMap::new(),
                command: &BTreeMap::new(),
                production: &BTreeMap::new(),
                nodes: &BTreeMap::new(),
                drop_offs: &BTreeMap::new(),
                spawns: &BTreeMap::new(),
                upgrades: &BTreeMap::new(),
            };
            self.units_file(additions, &RulesNames::default())
        }
    }
}
