use std::collections::{BTreeMap, btree_map};
use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_mode::ModeParam;
use serde::{Deserialize, Serialize};

use crate::files::script_list::ScriptSource;

/// The bots a mode names, by name, as its `data/mode.toml` gives them under `bots`, which a host
/// gives a seat by its name (08-sessions.md, A mode names its bots).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BotList(BTreeMap<DeclaredName, BotData>);

/// A bot: its script, the period of its think, and its params, which lie over the mode's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotData {
    pub script: ScriptSource,
    pub think_ms: NonZeroU32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<DeclaredName, ModeParam>,
}

impl BotList {
    /// The key the mode's file names its bots under.
    pub const KEY: &'static str = "bots";

    /// The bots of `bots`, each by its name.
    pub const fn new(bots: BTreeMap<DeclaredName, BotData>) -> Self {
        Self(bots)
    }

    /// Each bot, by its name, in the order of their names.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = (&DeclaredName, &BotData)> {
        self.0.iter()
    }

    /// The bot `name`; none for a name the mode does not give.
    pub(crate) fn get(&self, name: &DeclaredName) -> Option<&BotData> {
        self.0.get(name)
    }
}

impl<'a> IntoIterator for &'a BotList {
    type Item = (&'a DeclaredName, &'a BotData);
    type IntoIter = btree_map::Iter<'a, DeclaredName, BotData>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
