use std::sync::Arc;

use campfire_codec::{TomlError, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, SectionReaders};
use campfire_sim::Capability;
use campfire_values::Number;
use serde::Deserialize;
use serde::de::Error;

use crate::listed_effect::error::{EffectError, EffectRule};
use crate::listed_effect::listed_spec::{EffectTo, ListedSpec};
use crate::listed_effect::{EffectResolveCx, EffectRuleCx, ListedEffect, ListedEffects};
use crate::planned_effect::PlannedEffect;

/// One effect of an effect list, as data gives it: what it does, the effect of the key it is
/// under, and to whom: the unit the list reached, or with `to = "source"`, the acting unit.
#[derive(Debug, Clone)]
pub struct EffectData {
    pub does: Arc<dyn ListedEffect>,
    pub to: EffectTo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToName {
    Source,
}

impl EffectData {
    /// The key of an effect's table beside the effect's own.
    pub const TO: &'static str = "to";

    /// The effect of `table`: the table of exactly one key that a listed effect or a planned one
    /// takes, read as its module registers it, and `to` beside it.
    fn read(mut table: TomlTable, readers: &SectionReaders) -> Result<Self, TomlError> {
        let to = match table.take::<ToName>(Self::TO).transpose()? {
            Some(ToName::Source) => EffectTo::Source,
            None => EffectTo::Reached,
        };
        if let Some(unknown) = table.keys().find(|key| !Self::takes(key, readers)) {
            let expected = readers.listed_keys::<ListedEffects>();
            let expected = expected.chain(PlannedEffect::ALL.map(PlannedEffect::name));
            let expected: Vec<String> = expected
                .chain([Self::TO])
                .map(|key| format!("`{key}`"))
                .collect();
            return Err(TomlError::custom(format_args!(
                "unknown field `{unknown}`, expected one of {}",
                expected.join(", ")
            )));
        }
        let mut entries = table.into_entries();
        let (Some((key, value)), None) = (entries.next(), entries.next()) else {
            return Err(TomlError::custom("exactly one effect"));
        };
        let does = if let Some(read) = readers.listed::<ListedEffects>(&key) {
            read(value, readers)?
        } else {
            let planned = PlannedEffect::named(&key).expect("a key an effect takes");
            Arc::new(PlannedEffectData(planned))
        };
        Ok(Self { does, to })
    }

    /// Whether a listed effect or a planned one takes `key`.
    fn takes(key: &str, readers: &SectionReaders) -> bool {
        readers.listed::<ListedEffects>(key).is_some() || PlannedEffect::named(key).is_some()
    }

    /// The list `value` gives, an array of effects' tables, each read as `read` reads it.
    pub fn read_list(value: TomlValue, readers: &SectionReaders) -> Result<Vec<Self>, TomlError> {
        let tables = value.read::<Vec<TomlTable>>()?;
        tables
            .into_iter()
            .map(|table| Self::read(table, readers))
            .collect()
    }
}

// By hand, as a derive compares the trait objects by a move.
impl PartialEq for EffectData {
    fn eq(&self, other: &Self) -> bool {
        *self.does == *other.does && self.to == other.to
    }
}

impl Eq for EffectData {}

/// The lists an effect holds, which its own capability runs: those of each hit of what it
/// launches, `on_hit`, and those of its end, `on_end`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchLists {
    pub on_hit: Vec<EffectData>,
    pub on_end: Vec<EffectData>,
}

impl LaunchLists {
    /// The keys of the lists in an effect's table.
    pub const KEYS: [&'static str; 2] = ["on_hit", "on_end"];

    /// The lists `table` holds, taken out of it, each read with the release's `readers`: none
    /// for a list it does not hold.
    pub fn take(table: &mut TomlTable, readers: &SectionReaders) -> Result<Self, TomlError> {
        let [on_hit, on_end] = Self::KEYS.map(|key| {
            table
                .take_value(key)
                .map_or(Ok(Vec::new()), |list| EffectData::read_list(list, readers))
        });
        Ok(Self {
            on_hit: on_hit?,
            on_end: on_end?,
        })
    }
}

/// A planned effect as a list gives it, whatever its table holds: the load refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlannedEffectData(pub(crate) PlannedEffect);

impl ListedEffect for PlannedEffectData {
    fn needs(&self) -> Option<Capability> {
        None
    }

    fn numbers(&self) -> Vec<&Number> {
        Vec::new()
    }

    fn duration(&self) -> Option<&Number> {
        None
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        None
    }

    fn lists(&self) -> Option<&LaunchLists> {
        None
    }

    fn takes_point(&self) -> bool {
        false
    }

    fn rule(&self, _: &dyn EffectRuleCx) -> Result<(), BookError> {
        Ok(())
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        Err(cx.broken(EffectRule::Planned(self.0)))
    }
}
