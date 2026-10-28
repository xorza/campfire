use std::collections::BTreeMap;

use campfire_abilities::CastData;
use campfire_actions::{ActionKeys, ActionRange, CooldownHolder, RangeField, Targeting};
use campfire_areas::AreaDeliveryData;
use campfire_codec::{TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_values::{
    FilterTag, Number, Param, ParamRef, Ranked, Scalar, Scaling, Stat, TagFilterData, TextId,
};
use serde::Serialize;

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::object_ini::ini_power::IniAutoHeal;
use crate::zero_hour::import_name::ImportName;

/// The param of a power's cast that its heal reads, its building's stat of the power's heal.
const HEAL: &str = "heal";

/// The section of a special power the import converts to a cast of its building (generals.md, A
/// power is a cast of its building; generals.md, The import converts a marker's burst): the area
/// of its marker's type it lands, which heals each unit it reaches by its building's stat of the
/// power's heal; its keys cast it at a point anywhere, its cooldown its player's, of its
/// `ReloadTime`, while its building holds the power's tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PowerCast {
    area: DeclaredName,
}

/// The names a power's conversion gives: its cast's id, the tag its building holds while its
/// player has the power's science, and the stat of its heal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PowerNames {
    pub(crate) id: DeclaredName,
    pub(crate) tag: DeclaredName,
    pub(crate) heal: Stat,
}

/// What a perk's modifier gives its player's units for a power: the power's tag, for its first
/// science, and what it adds to the power's heal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct PowerModifier {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) stats: BTreeMap<Stat, Number>,
}

impl PowerNames {
    /// The names of the power `power`, as the game names it; an error for one past the most a name
    /// holds.
    pub(crate) fn of(power: &str) -> Result<Self, NameError> {
        let name = ImportName::latin(power);
        let id = name.id("power")?;
        let heal = format!("{}_heal", id.as_str());
        let heal = Stat::named(&heal).ok_or(NameError::TooLong(heal))?;
        Ok(Self {
            tag: id.clone(),
            id,
            heal,
        })
    }
}

impl PowerCast {
    /// The cast that lands an area of the type `area`.
    pub(crate) const fn new(area: DeclaredName) -> Self {
        Self { area }
    }

    /// The keys of the cast of a power of `names`, ready again `reload_ms` after a use, its
    /// message `message`.
    pub(crate) fn keys(names: &PowerNames, reload_ms: u32, message: &str) -> ActionKeys {
        let tag = FilterTag {
            name: names.tag.clone(),
            negated: false,
        };
        let heal = Scaling {
            base: Ranked::One(Num::ZERO),
            per_level: Num::ZERO,
            bonus: BTreeMap::new(),
            ratios: BTreeMap::from([(names.heal.clone(), Num::ONE)]),
        };
        ActionKeys {
            name: TextId::new(message),
            range: Some(Ranked::One(RangeField::Range(ActionRange::Global))),
            cooldown_ms: Some(Ranked::One(Number::from(i64::from(reload_ms)))),
            cooldown_holder: CooldownHolder::Player,
            when: Some(TagFilterData::new([tag])),
            params: BTreeMap::from([(
                DeclaredName::new(HEAL).expect("a param's name"),
                Param::Scaling(heal),
            )]),
            ..ActionKeys::new(Targeting::Point)
        }
    }

    /// Its table under `[actions]`: `keys`, a cast's section, its area's delivery, and the heal
    /// each unit its area reaches takes.
    pub(crate) fn table(&self, keys: &ActionKeys) -> TomlTable {
        #[derive(Debug, Serialize)]
        struct OnHit<'a> {
            heal: Heal<'a>,
        }
        #[derive(Debug, Serialize)]
        struct Heal<'a> {
            amount: &'a Number,
        }
        let delivery = AreaDeliveryData {
            area: self.area.clone(),
        };
        let mut table = keys.table_delivering(&CastData {}, &delivery);
        let amount = Number::Param(ParamRef {
            param: DeclaredName::new(HEAL).expect("a param's name"),
        });
        let on_hit = [OnHit {
            heal: Heal { amount: &amount },
        }];
        table.insert("on_hit".to_owned(), TomlValue::of(&on_hit));
        table
    }
}

impl PowerModifier {
    /// The modifiers of the perks of a power's sciences, by the perks' names: its first science's,
    /// `first`, gives its tag and its marker's heal, and each of `upgrades`, a science and its
    /// marker's heal, adds what its heal holds past the one below it.
    pub(crate) fn of(
        names: &PowerNames,
        first: (DeclaredName, &IniAutoHeal),
        upgrades: &[(DeclaredName, &IniAutoHeal)],
    ) -> BTreeMap<DeclaredName, Self> {
        let added = |amount: i32| {
            let amount = Number::Value(Scalar::Int(i64::from(amount)));
            BTreeMap::from([(names.heal.clone(), amount)])
        };
        let (science, heal) = first;
        let mut modifiers = BTreeMap::from([(
            science,
            Self {
                tags: vec![names.tag.clone()],
                stats: added(heal.amount),
            },
        )]);
        let mut ordered: Vec<_> = upgrades.iter().collect();
        ordered.sort_by_key(|(_, heal)| heal.amount);
        let mut below = heal.amount;
        for (science, heal) in ordered {
            modifiers.insert(
                science.clone(),
                Self {
                    tags: Vec::new(),
                    stats: added(heal.amount - below),
                },
            );
            below = heal.amount;
        }
        modifiers
    }
}
