use std::collections::BTreeMap;

use campfire_content::PackagePath;
use serde::Deserialize;

use crate::stats::modifier_data::ModifierData;
use crate::units::scalar::Scalar;
use crate::units::state_decl::StateDecl;

/// The mode's `data/mode.toml`: its script, its player inputs, its state and its params.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeData {
    pub script: PackagePath,
    /// How long after damage a unit counts as having assisted a kill.
    pub assist_window_ms: Option<u64>,
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
