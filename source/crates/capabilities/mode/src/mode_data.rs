use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use campfire_actions::SlotKinds;
use campfire_common::DeclaredName;
use campfire_declarations::{
    PatchError, Patched, Section, SectionReaders, SectionTable, Sections, TableRead,
};
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
/// sections of the modules that read the rest. It reads through its patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeData {
    /// The message of the name players see.
    pub name: TextId,
    /// What its players choose before their units spawn, by name.
    pub choices: BTreeMap<DeclaredName, ChoiceData>,
    /// The type of each player input, by name. An input that does not match its type never
    /// reaches the script.
    pub inputs: BTreeMap<DeclaredName, InputType>,
    /// The names its script's timers take, in order.
    pub timers: BTreeSet<DeclaredName>,
    /// The params of its maps' markers its scripts read, which a map of the mode may lack, as a
    /// script serves every map of its mode.
    pub marker_params: BTreeSet<DeclaredName>,
    pub state_version: Option<u32>,
    pub state: BTreeMap<DeclaredName, StateFieldDecl>,
    /// The fields of each player's state, by name.
    pub player_state: BTreeMap<DeclaredName, StateFieldDecl>,
    pub params: BTreeMap<DeclaredName, ModeParam>,
    /// The params whose values a session's host picks, by name.
    pub options: BTreeMap<DeclaredName, OptionData>,
    /// Each of its manifest's teams, by name.
    pub teams: BTreeMap<DeclaredName, TeamData>,
    /// How pairs of its teams regard each other; a pair not named is hostile.
    pub relations: Vec<RelationData>,
    /// Who may take a slot once the match started.
    pub players: PlayersData,
    /// Who may ask for a save, and how often the mode saves.
    pub saves: SavesData,
    /// Who receives each kind of its state, and what it leaves to the host.
    pub visibility: ModeVisibility,
    /// Read apart, by `ModeData::seed`.
    pub sections: Sections,
}

/// The mode's own keys as a tool writes them in part, over the rules package's data the mode's
/// lies on: each of `ModeData`'s, but its sections, optional (03-game-scripting.md, A
/// section has a partial form).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<TextId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<BTreeMap<DeclaredName, ChoiceData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<BTreeMap<DeclaredName, InputType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timers: Option<BTreeSet<DeclaredName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_params: Option<BTreeSet<DeclaredName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<BTreeMap<DeclaredName, StateFieldDecl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_state: Option<BTreeMap<DeclaredName, StateFieldDecl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<DeclaredName, ModeParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BTreeMap<DeclaredName, OptionData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams: Option<BTreeMap<DeclaredName, TeamData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<RelationData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<PlayersData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saves: Option<SavesData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ModeVisibility>,
}

impl Patched for ModeData {
    type Patch = ModePatch;

    fn from_patch(patch: ModePatch) -> Result<Self, PatchError> {
        Ok(Self {
            name: patch.name.ok_or(PatchError::Missing("name"))?,
            choices: patch.choices.unwrap_or_default(),
            inputs: patch.inputs.unwrap_or_default(),
            timers: patch.timers.unwrap_or_default(),
            marker_params: patch.marker_params.unwrap_or_default(),
            state_version: patch.state_version,
            state: patch.state.unwrap_or_default(),
            player_state: patch.player_state.unwrap_or_default(),
            params: patch.params.unwrap_or_default(),
            options: patch.options.unwrap_or_default(),
            teams: patch.teams.unwrap_or_default(),
            relations: patch.relations.unwrap_or_default(),
            players: patch.players.unwrap_or_default(),
            saves: patch.saves.unwrap_or_default(),
            visibility: patch.visibility.unwrap_or_default(),
            sections: Sections::default(),
        })
    }
}

impl<'de> Deserialize<'de> for ModeData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    String,
    StringList,
}

/// A mode param: a value, a name, a list, or a text, as `ctx.params` reads it in the mode's, the
/// AI's and the bots' scripts, and as a marker's params and a bot's own hold it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModeParam {
    Value(Scalar),
    /// A name the mode declares, such as a unit type's, which no decimal spells.
    Name(DeclaredName),
    List(Vec<ListEntry>),
    /// A string that is neither a decimal nor a declared name, such as a waypoint's.
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
                        ListEntry::Name(name) => text(name.as_str()),
                        ListEntry::Text(entry) => text(entry),
                    })
                    .collect(),
            ),
            Self::Name(name) => text(name.as_str()),
            Self::Text(value) => text(value),
        }
    }
}

/// An entry of a list param: a value, a declared name, or a string that is neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ListEntry {
    Value(Scalar),
    Name(DeclaredName),
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

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_param_reads_as_a_value_then_a_name_then_a_text() {
        #[derive(Debug, Deserialize)]
        struct Params {
            params: BTreeMap<DeclaredName, ModeParam>,
        }
        let Params { params } = Toml::parse(
            r#"params = { a = 2, b = "0.5", c = "tank", d = "Fixture_Center", e = ["tank", "12", "X y"] }"#,
        )
        .unwrap();
        let name = |text| DeclaredName::new(text).unwrap();
        // A decimal's string is a value, a decimal even for a whole number; a declared name's,
        // which starts with a letter, a name; any other string a text.
        assert_eq!(
            params.into_values().collect::<Vec<_>>(),
            [
                ModeParam::Value(Scalar::Int(2)),
                ModeParam::Value(Scalar::Decimal(Num::ONE / Num::int(2))),
                ModeParam::Name(name("tank")),
                ModeParam::Text("Fixture_Center".to_owned()),
                ModeParam::List(vec![
                    ListEntry::Name(name("tank")),
                    ListEntry::Value(Scalar::Decimal(Num::int(12))),
                    ListEntry::Text("X y".to_owned()),
                ]),
            ]
        );
    }
}
