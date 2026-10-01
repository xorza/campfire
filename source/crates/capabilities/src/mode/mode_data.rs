use std::collections::BTreeMap;

use campfire_content::PackagePath;
use campfire_script::rhai::{Dynamic, ImmutableString};
use serde::Deserialize;

use crate::combat::combat_rules::CombatRules;
use crate::mode::relation_data::RelationData;
use crate::navigation::navigation_rules::NavigationRules;
use crate::scripts::state_decl::StateDecl;
use crate::stats::modifier_data::ModifierData;
use crate::stats::pool_data::PoolData;
use crate::stats::stat::Stat;
use crate::stats::stat_rule::StatRule;
use crate::units::tag_data::TagData;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// The mode's `data/mode.toml`: its script, its player inputs, its state and its params.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeData {
    pub script: PackagePath,
    #[serde(default)]
    pub combat: CombatRules,
    #[serde(default)]
    pub navigation: NavigationRules,
    /// The type of each player input, by name. An input that does not match its type never
    /// reaches the script.
    #[serde(default)]
    pub inputs: BTreeMap<String, InputType>,
    pub state_version: Option<u32>,
    #[serde(default)]
    pub state: BTreeMap<String, StateDecl>,
    #[serde(default)]
    pub params: BTreeMap<String, ModeParam>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
    /// The kind of damage every attack deals, one of `[combat] damage_kinds`.
    pub attack_kind: Option<DeclaredName>,
    /// Every stat its units carry, those the engine reads among them, each with its rule.
    #[serde(default)]
    pub stats: BTreeMap<Stat, StatRule>,
    /// Its pools, by name, the life pool among them: the amounts its units spend and lose.
    #[serde(default)]
    pub pools: BTreeMap<DeclaredName, PoolData>,
    /// Its players' resources, such as gold, which share no name with a pool.
    #[serde(default)]
    pub resources: Vec<DeclaredName>,
    /// How pairs of its teams regard each other; a pair not named is hostile.
    #[serde(default)]
    pub relations: Vec<RelationData>,
    /// The effects of its tags, by name.
    #[serde(default)]
    pub tags: BTreeMap<String, TagData>,
}

/// The type of a player input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    String,
    StringList,
}

/// A mode param: a value, a list, or a text, as `ctx.p` reads it in the mode's and the AI's
/// scripts, and as a marker's params hold it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ModeParam {
    Value(Scalar),
    List(Vec<ListEntry>),
    /// A string that is not a decimal, such as a unit type.
    Text(String),
}

impl ModeParam {
    /// The param as a script reads it.
    pub(crate) fn to_dynamic(&self) -> Dynamic {
        let text = |text: &str| Dynamic::from(ImmutableString::from(text));
        match self {
            ModeParam::Value(value) => value.to_dynamic(),
            ModeParam::List(entries) => Dynamic::from_array(
                entries
                    .iter()
                    .map(|entry| match entry {
                        ListEntry::Value(value) => value.to_dynamic(),
                        ListEntry::Text(entry) => text(entry),
                    })
                    .collect(),
            ),
            ModeParam::Text(value) => text(value),
        }
    }
}

/// An entry of a list param: a value, or a string that is not a decimal, such as a unit type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ListEntry {
    Value(Scalar),
    Text(String),
}
