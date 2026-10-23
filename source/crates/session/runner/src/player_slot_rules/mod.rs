use campfire_common::AfterLeave;
use campfire_mode::PlayersData;
use campfire_protocol::{PlayerSlotChangeKind, Taken};

use crate::player_slot_rules::error::PlayerSlotRuleError;

pub(crate) mod error;

/// The mode's rules of who may take a player slot, as `[players]` gives them, which every change of
/// a player slot's controller in a session of the mode keeps: the server's as it logs them, and a
/// verifier's as it replays a published log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerSlotRules(PlayersData);

impl PlayerSlotRules {
    pub(crate) const fn new(players: PlayersData) -> Self {
        Self(players)
    }

    /// What a leaver's player slot becomes, as the mode's `leaver` says.
    pub(crate) const fn after_leave(self) -> AfterLeave {
        self.0.leaver
    }

    /// Whether the mode allows `change`: a player who takes back their own player slot always; a
    /// new one an open player slot with late join, a bot's with bot takeover; a leaver's player
    /// slot only as `leaver` says.
    pub(crate) const fn check(
        self,
        change: PlayerSlotChangeKind,
    ) -> Result<(), PlayerSlotRuleError> {
        let PlayersData {
            late_join,
            bot_takeover,
            leaver,
            control: _,
        } = self.0;
        match change {
            PlayerSlotChangeKind::Joined { from: Taken::Open } if !late_join => {
                Err(PlayerSlotRuleError::LateJoin)
            }
            PlayerSlotChangeKind::Joined { from: Taken::Bot } if !bot_takeover => {
                Err(PlayerSlotRuleError::BotTakeover)
            }
            PlayerSlotChangeKind::Joined { .. } => Ok(()),
            PlayerSlotChangeKind::Left { becomes } => match (leaver, becomes) {
                (AfterLeave::Reserve, AfterLeave::Reserve)
                | (AfterLeave::Bot, AfterLeave::Bot)
                | (AfterLeave::Open, AfterLeave::Open) => Ok(()),
                _ => Err(PlayerSlotRuleError::Leaver { becomes }),
            },
        }
    }
}

#[cfg(test)]
mod tests;
