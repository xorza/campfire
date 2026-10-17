use std::borrow::Cow;
use std::collections::BTreeMap;
use std::{fmt, iter};

use campfire_actions::{ActionData, ActionsData};
use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionReaders, SectionTable, Sections, TableRead, UnitTypeData};
use campfire_items::{ItemData, ItemsData};
use campfire_stats::{ModifierData, ModifiersData};
use serde::Deserializer;
use serde::de::{DeserializeSeed, MapAccess, Visitor};

use crate::books::units_seed::UnitsSeed;

/// What a package holds for a match: its unit types by name, and the sections of the modules that
/// read the rest, its actions, modifiers and item types by id. The mode, an avatar and a loadout
/// hold the same shape, and the load lets only the mode hold item types.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageContent {
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
    pub sections: Sections,
}

impl PackageContent {
    const UNITS: &'static str = "units";

    /// The keys of a data file's table that hold a package's content, as `readers` register its
    /// sections.
    pub fn keys(readers: &SectionReaders) -> impl Iterator<Item = &'static str> {
        iter::once(Self::UNITS).chain(readers.keys(SectionTable::Package))
    }

    /// What reads a package's content, its sections and its unit types' as `readers` register
    /// them.
    pub const fn seed(readers: &SectionReaders) -> ContentSeed<'_> {
        ContentSeed(readers)
    }

    /// Its section `S`, if it gives it.
    pub fn section<S: Section>(&self) -> Option<&S> {
        debug_assert_eq!(S::TABLE, SectionTable::Package, "{}", S::KEY);
        self.sections.get()
    }

    /// Its actions, by id: none when it gives no `[actions]`.
    pub fn actions(&self) -> &BTreeMap<DeclaredName, ActionData> {
        static NONE: ActionsData = ActionsData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }

    /// Its modifiers, by id: none when it gives no `[modifiers]`.
    pub fn modifiers(&self) -> &BTreeMap<DeclaredName, ModifierData> {
        static NONE: ModifiersData = ModifiersData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }

    /// Its item types, by id: none when it gives no `[items]`.
    pub fn items(&self) -> &BTreeMap<DeclaredName, ItemData> {
        static NONE: ItemsData = ItemsData(BTreeMap::new());
        &self.section().unwrap_or(&NONE).0
    }

    /// Its section `S`, or the value a package that does not give it reads as.
    pub fn section_or_default<S: Section + Default>(&self) -> Cow<'_, S> {
        debug_assert_eq!(S::TABLE, SectionTable::Package, "{}", S::KEY);
        self.sections.get_or_default()
    }
}

/// What reads a package's content, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct ContentSeed<'r>(&'r SectionReaders);

impl<'de> DeserializeSeed<'de> for ContentSeed<'_> {
    type Value = PackageContent;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<PackageContent, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ContentSeed<'_> {
    type Value = PackageContent;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a package's content")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<PackageContent, A::Error> {
        let readers = self.0;
        let table = readers.table(SectionTable::Package);
        let mut read = TableRead::default();
        let mut units = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if key == PackageContent::UNITS {
                units = map.next_value_seed(UnitsSeed(readers))?;
            } else if !table.next_section(&key, &mut map, &mut read)? {
                return Err(table.unknown(&key, &[PackageContent::UNITS]));
            }
        }
        Ok(PackageContent {
            units,
            sections: read.sections,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use campfire_actions::{EffectData, EffectTo, LaunchLists, ListedEffect};
    use campfire_areas::LaunchEffectData;
    use campfire_codec::{Toml, TomlTable};
    use campfire_combat::HealEffectData;
    use campfire_progression::XpEffectData;
    use campfire_values::{Number, Scalar};

    use super::*;
    use crate::release::Release;

    #[test]
    fn a_package_reads_its_unit_types_and_its_sections_and_no_other_key() {
        let readers = Release::sections();
        let read = |text: &str| {
            let raw = Toml::parse::<TomlTable>(text).unwrap();
            raw.read_with(PackageContent::seed(&readers))
        };
        let content = read("[units.scout]\ntags = [\"fast\"]\n[actions]\n[modifiers]").unwrap();
        let units: Vec<&str> = content.units.keys().map(DeclaredName::as_str).collect();
        assert_eq!(units, ["scout"]);
        assert_eq!(
            content.section::<ActionsData>(),
            Some(&ActionsData::default())
        );
        assert_eq!(
            PackageContent::keys(&readers).collect::<Vec<_>>(),
            ["units", "actions", "modifiers", "items"]
        );
        assert_eq!(
            read("[unit]").unwrap_err().message(),
            "unknown field `unit`, expected one of `units`, `actions`, `modifiers`, `items`"
        );
        // An action's lists read each effect as the module of its key registers it, a launch's
        // own lists among them, and `to` beside it.
        let bolt = |list: &str| {
            let text = format!("[actions.bolt]\ntargeting = \"none\"\non_resolve = {list}");
            read(&text).map(|content| {
                let actions = content.section::<ActionsData>().unwrap();
                actions.0[&DeclaredName::new("bolt").unwrap()]
                    .on_resolve
                    .clone()
            })
        };
        let name = |name: &str| DeclaredName::new(name).unwrap();
        let one = || Number::Value(Scalar::Int(1));
        let heal: Arc<dyn ListedEffect> = Arc::new(HealEffectData { amount: one() });
        let xp: Arc<dyn ListedEffect> = Arc::new(XpEffectData {
            track: name("valor"),
            amount: one(),
        });
        let launch: Arc<dyn ListedEffect> = Arc::new(LaunchEffectData {
            area: name("blast"),
            lists: LaunchLists {
                on_hit: Vec::new(),
                on_end: vec![EffectData {
                    does: xp,
                    to: EffectTo::Reached,
                }],
            },
        });
        let list = r#"[{ heal = { amount = 1 }, to = "source" }, { launch = { area = "blast", on_end = [{ xp = { track = "valor", amount = 1 } }] } }]"#;
        assert_eq!(
            bolt(list).unwrap(),
            [
                EffectData {
                    does: heal,
                    to: EffectTo::Source,
                },
                EffectData {
                    does: launch,
                    to: EffectTo::Reached,
                },
            ]
        );
        // Exactly one effect's key, of a module or planned, at every depth.
        let unknown = "unknown field `bogus`, expected one of `spawn`, `modifier`, `purge`, `restore`, `xp`, `damage`, `heal`, `move`, `launch`, `loot`, `noise`, `to`";
        for (list, message) in [
            ("[{}]", "exactly one effect"),
            (r#"[{ to = "source" }]"#, "exactly one effect"),
            (
                "[{ heal = { amount = 1 }, loot = {} }]",
                "exactly one effect",
            ),
            ("[{ bogus = {} }]", unknown),
            (
                r#"[{ launch = { area = "blast", on_hit = [{ bogus = {} }] } }]"#,
                unknown,
            ),
        ] {
            let error = bolt(list).unwrap_err();
            assert_eq!(error.message(), message, "{list}");
        }
        assert!(bolt("[{ loot = { anything = 1 } }]").is_ok());
    }
}
