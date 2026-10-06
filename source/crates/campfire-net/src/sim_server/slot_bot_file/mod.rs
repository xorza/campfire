use std::path::PathBuf;
use std::str::FromStr;

use crate::order_script::OrderScript;
use crate::order_script::error::OrderScriptReadError;
use crate::sim_server::server_bots::SlotBot;
use crate::sim_server::slot_bot_file::error::SlotBotFileError;

pub(crate) mod error;

/// A server's bot as a command line names it, `<slot>=<orders file>`: the index of the slot it
/// plays, and the file of its order script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotBotFile {
    pub slot: u32,
    pub path: PathBuf,
}

impl SlotBotFile {
    /// The bot, its script read from its file.
    pub fn read(&self) -> Result<SlotBot, OrderScriptReadError> {
        Ok(SlotBot::new(self.slot, OrderScript::read(&self.path)?))
    }
}

impl FromStr for SlotBotFile {
    type Err = SlotBotFileError;

    fn from_str(text: &str) -> Result<SlotBotFile, SlotBotFileError> {
        let (slot, path) = text
            .split_once('=')
            .ok_or_else(|| SlotBotFileError::NotPair(text.to_owned()))?;
        let slot = slot.parse().map_err(|error| SlotBotFileError::Slot {
            text: slot.to_owned(),
            error,
        })?;
        Ok(SlotBotFile {
            slot,
            path: PathBuf::from(path),
        })
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test makes and removes the files of its fixtures"
)]
#[cfg(test)]
mod tests;
