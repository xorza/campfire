use std::str::FromStr;

use campfire_common::{DeclaredName, PlayerSlot};

use crate::error::PlayerSlotPickTextError;

/// A host's pick of a player slot's choice by its value, as `--pick` writes it:
/// `player_slot:choice=value` (08-sessions.md, The terms hold each player slot's picks). The mode's
/// choices check it as a session opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSlotPick {
    pub player_slot: PlayerSlot,
    pub choice: DeclaredName,
    pub value: DeclaredName,
}

impl FromStr for PlayerSlotPick {
    type Err = PlayerSlotPickTextError;

    fn from_str(text: &str) -> Result<Self, PlayerSlotPickTextError> {
        let (player_slot, rest) = text.split_once(':').ok_or(PlayerSlotPickTextError::Form)?;
        let (choice, value) = rest.split_once('=').ok_or(PlayerSlotPickTextError::Form)?;
        let player_slot = player_slot
            .parse()
            .map(PlayerSlot::new)
            .map_err(|_| PlayerSlotPickTextError::PlayerSlot(player_slot.to_owned()))?;
        let choice = DeclaredName::new(choice)
            .ok_or_else(|| PlayerSlotPickTextError::Choice(choice.to_owned()))?;
        let value = DeclaredName::new(value)
            .ok_or_else(|| PlayerSlotPickTextError::Value(value.to_owned()))?;
        Ok(Self {
            player_slot,
            choice,
            value,
        })
    }
}

#[cfg(test)]
mod tests;
