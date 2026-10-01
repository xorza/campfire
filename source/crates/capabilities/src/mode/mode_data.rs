use std::collections::BTreeMap;

use campfire_content::PackagePath;
use serde::Deserialize;

use crate::combat::combat_rules::CombatRules;
use crate::scripts::state_decl::StateDecl;
use crate::stats::modifier_data::ModifierData;
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
    /// What its units spend on abilities, such as mana or energy.
    #[serde(default)]
    pub resources: Vec<DeclaredName>,
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

/// A mode param: a value, or a list, as `ctx.p` reads it in the mode's and the AI's scripts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ModeParam {
    Value(Scalar),
    List(Vec<ListEntry>),
}

/// An entry of a list param: a value, or a string that is not a decimal, such as a unit type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ListEntry {
    Value(Scalar),
    Text(String),
}
