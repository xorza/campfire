use std::collections::BTreeMap;

use serde::Deserialize;

use crate::actions::slot_kinds::SlotKinds;
use crate::combat::mode_combat_data::ModeCombatData;
use crate::items::shop_data::ShopData;
use crate::mode::choice_data::ChoiceData;
use crate::mode::players_data::PlayersData;
use crate::mode::relation_data::RelationData;
use crate::mode::saves_data::SavesData;
use crate::navigation::mode_navigation_data::ModeNavigationData;
use crate::production::mode_supply_data::ModeSupplyData;
use crate::progression::track_data::TrackData;
use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_list::ScriptList;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::stats::pool_data::PoolData;
use crate::stats::stat_rule::StatRule;
use crate::units::tag_data::TagData;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;
use crate::visibility::mode_visibility::ModeVisibility;

/// The mode's `data/mode.toml`, but its script, which its package's load resolves, and its
/// actions, modifiers and item types, which its package's content holds: its player inputs, its
/// state, its params and its shop.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeData {
    #[serde(default)]
    pub combat: ModeCombatData,
    #[serde(default)]
    pub navigation: ModeNavigationData,
    /// The kinds of slot its units' actions sit in, in order.
    #[serde(default)]
    pub slots: SlotKinds,
    /// What its players choose before their units spawn, by name.
    #[serde(default)]
    pub choices: BTreeMap<DeclaredName, ChoiceData>,
    /// The type of each player input, by name. An input that does not match its type never
    /// reaches the script.
    #[serde(default)]
    pub inputs: BTreeMap<DeclaredName, InputType>,
    pub state_version: Option<u32>,
    #[serde(default)]
    pub state: BTreeMap<DeclaredName, SyncedStateDecl>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, ModeParam>,
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
    /// The properties of its tags, by name.
    #[serde(default)]
    pub tags: BTreeMap<DeclaredName, TagData>,
    /// The tracks its units gain experience on, by name.
    #[serde(default)]
    pub tracks: BTreeMap<DeclaredName, TrackData>,
    /// Where its units buy and sell items.
    pub shop: Option<ShopData>,
    /// Who may take a slot once the match started.
    #[serde(default)]
    pub players: PlayersData,
    /// Who may ask for a save, and how often the mode saves.
    #[serde(default)]
    pub saves: SavesData,
    /// Whether it counts supply, and its most.
    pub supply: Option<ModeSupplyData>,
    /// Who receives each kind of its state, and what it leaves to the host.
    #[serde(default)]
    pub visibility: ModeVisibility,
}

impl ModeData {
    /// The ranks of every loadout entry: those of the slot kind a choice of loadout entries
    /// fills, which the load checked they all share, or 1 with none.
    pub fn loadout_ranks(&self) -> u8 {
        let mut kinds = self
            .choices
            .values()
            .filter_map(|choice| choice.slot.as_ref());
        let kind = kinds.find_map(|kind| self.slots.names().named(kind.as_str()));
        kind.map_or(1, |kind| self.slots.ranks(kind))
    }
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
    pub(crate) fn to_value(&self) -> ScriptValue {
        let text = |text: &str| ScriptValue::of(ScriptText::from(text));
        match self {
            ModeParam::Value(value) => ScriptValue::from(*value),
            ModeParam::List(entries) => ScriptValue::of::<ScriptList>(
                entries
                    .iter()
                    .map(|entry| match entry {
                        ListEntry::Value(value) => ScriptValue::from(*value),
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

impl DataSection for ModeData {
    const TABLE: DataTable = DataTable::Mode;
}
