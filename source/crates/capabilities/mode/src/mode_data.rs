use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use campfire_actions::SlotKinds;
use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionReaders, SectionTable, Sections, TableRead};
use campfire_script::{
    DataSection, DataTable, ScriptList, ScriptText, ScriptValue, StateFieldDecl,
};
use campfire_values::{Scalar, TextId};
use campfire_visibility::ModeVisibility;
use serde::de::DeserializeSeed;
use serde::{Deserialize, Deserializer, Serialize};

use crate::choice_data::ChoiceData;
use crate::option_data::OptionData;
use crate::players_data::PlayersData;
use crate::relation_data::RelationData;
use crate::saves_data::SavesData;
use crate::team_data::TeamData;

/// The mode's `data/mode.toml`, but its script, which its package's load resolves, and its
/// actions, modifiers and item types, which its package's content holds: its name, its teams'
/// names, its player inputs, its state, its params and its options, the mode's own, and the
/// sections of the modules that read the rest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeData {
    /// The message of the name players see.
    pub name: TextId,
    /// What its players choose before their units spawn, by name.
    #[serde(default)]
    pub choices: BTreeMap<DeclaredName, ChoiceData>,
    /// The type of each player input, by name. An input that does not match its type never
    /// reaches the script.
    #[serde(default)]
    pub inputs: BTreeMap<DeclaredName, InputType>,
    /// The names its script's timers take, in order.
    #[serde(default)]
    pub timers: BTreeSet<DeclaredName>,
    /// The params of its maps' markers its scripts read, which a map of the mode may lack, as a
    /// script serves every map of its mode.
    #[serde(default)]
    pub marker_params: BTreeSet<DeclaredName>,
    pub state_version: Option<u32>,
    #[serde(default)]
    pub state: BTreeMap<DeclaredName, StateFieldDecl>,
    /// The fields of each player's state, by name.
    #[serde(default)]
    pub player_state: BTreeMap<DeclaredName, StateFieldDecl>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, ModeParam>,
    /// The params whose values a session's host picks, by name.
    #[serde(default)]
    pub options: BTreeMap<DeclaredName, OptionData>,
    /// Each of its manifest's teams, by name.
    #[serde(default)]
    pub teams: BTreeMap<DeclaredName, TeamData>,
    /// How pairs of its teams regard each other; a pair not named is hostile.
    #[serde(default)]
    pub relations: Vec<RelationData>,
    /// Who may take a slot once the match started.
    #[serde(default)]
    pub players: PlayersData,
    /// Who may ask for a save, and how often the mode saves.
    #[serde(default)]
    pub saves: SavesData,
    /// Who receives each kind of its state, and what it leaves to the host.
    #[serde(default)]
    pub visibility: ModeVisibility,
    /// Read apart, by `ModeData::seed`.
    #[serde(skip)]
    pub sections: Sections,
}

impl ModeData {
    /// The key of its `[players]`.
    pub const PLAYERS: &'static str = "players";
    /// The key of its state fields.
    pub const STATE: &'static str = "state";
    /// The key of its options.
    pub const OPTIONS: &'static str = "options";
    /// The key of its choices.
    pub const CHOICES: &'static str = "choices";

    /// What reads the mode's table, its sections as `readers` register them; an error for a key
    /// neither the mode nor a section reads, or a value that is no value of its type.
    pub const fn seed(readers: &SectionReaders) -> ModeSeed<'_> {
        ModeSeed(readers)
    }

    /// Its section `S`, if it gives it.
    pub fn section<S: Section>(&self) -> Option<&S> {
        debug_assert_eq!(S::TABLE, SectionTable::Mode, "{}", S::KEY);
        self.sections.get()
    }

    /// Its section `S`, or the value a mode that does not give it reads as.
    pub fn section_or_default<S: Section + Default>(&self) -> Cow<'_, S> {
        debug_assert_eq!(S::TABLE, SectionTable::Mode, "{}", S::KEY);
        self.sections.get_or_default()
    }

    /// The names `ctx.params` reads of the mode: its params', then its options'.
    pub fn param_names(&self) -> impl Iterator<Item = &DeclaredName> {
        self.params.keys().chain(self.options.keys())
    }

    /// The ranks of every loadout entry: those of the slot kind a choice of loadout entries
    /// fills, which the load checked they all share, or 1 with none.
    pub fn loadout_ranks(&self) -> u8 {
        let mut kinds = self
            .choices
            .values()
            .filter_map(|choice| choice.slot.as_ref());
        let slots = self.section_or_default::<SlotKinds>();
        let kind = kinds.find_map(|kind| slots.names().named(kind.as_str()));
        kind.map_or(1, |kind| slots.ranks(kind))
    }
}

/// What reads the mode's table, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct ModeSeed<'r>(&'r SectionReaders);

impl<'de> DeserializeSeed<'de> for ModeSeed<'_> {
    type Value = ModeData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<ModeData, D::Error> {
        let table = self.0.table(SectionTable::Mode);
        let TableRead { sections, rest, .. } = table.deserialize(deserializer)?;
        let data: ModeData = TableRead::own(rest)?;
        Ok(ModeData { sections, ..data })
    }
}

/// The type of a player input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    String,
    StringList,
}

/// A mode param: a value, a list, or a text, as `ctx.params` reads it in the mode's and the AI's
/// scripts, and as a marker's params hold it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModeParam {
    Value(Scalar),
    List(Vec<ListEntry>),
    /// A string that is not a decimal, such as a unit type.
    Text(String),
}

impl ModeParam {
    /// The param as a script reads it.
    pub fn to_value(&self) -> ScriptValue {
        let text = |text: &str| ScriptValue::of(ScriptText::from(text));
        match self {
            Self::Value(value) => ScriptValue::from(*value),
            Self::List(entries) => ScriptValue::of::<ScriptList>(
                entries
                    .iter()
                    .map(|entry| match entry {
                        ListEntry::Value(value) => ScriptValue::from(*value),
                        ListEntry::Text(entry) => text(entry),
                    })
                    .collect(),
            ),
            Self::Text(value) => text(value),
        }
    }
}

/// An entry of a list param: a value, or a string that is not a decimal, such as a unit type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ListEntry {
    Value(Scalar),
    Text(String),
}

impl DataSection for ModeData {
    const TABLE: DataTable = DataTable::MODE;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::{BTreeMap, BTreeSet};

    use campfire_declarations::{Section, SectionTable, Sections};
    use campfire_values::TextId;
    use campfire_visibility::ModeVisibility;

    use crate::mode_data::ModeData;
    use crate::players_data::PlayersData;
    use crate::saves_data::SavesData;

    impl ModeData {
        /// The data of a mode of the name `name` and nothing more: no team, input, timer, state,
        /// param or section.
        pub fn named(name: TextId) -> Self {
            Self {
                name,
                choices: BTreeMap::new(),
                inputs: BTreeMap::new(),
                timers: BTreeSet::new(),
                marker_params: BTreeSet::new(),
                state_version: None,
                state: BTreeMap::new(),
                player_state: BTreeMap::new(),
                params: BTreeMap::new(),
                options: BTreeMap::new(),
                teams: BTreeMap::new(),
                relations: Vec::new(),
                players: PlayersData::default(),
                saves: SavesData::default(),
                visibility: ModeVisibility::default(),
                sections: Sections::default(),
            }
        }

        /// It with `section` too, in place of one of its type.
        #[must_use]
        pub fn with<S: Section>(self, section: S) -> Self {
            debug_assert_eq!(S::TABLE, SectionTable::Mode, "{}", S::KEY);
            Self {
                sections: self.sections.with(section),
                ..self
            }
        }
    }
}
