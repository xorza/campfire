use campfire_common::AfterLeave;
use campfire_mode::PlayersData;
use campfire_protocol::{SlotChangeKind, Taken};

use crate::slot_rules::error::SlotRuleError;

pub(crate) mod error;

/// The mode's rules of who may take a slot, as `[players]` gives them, which every change of a
/// slot's controller in a session of the mode keeps: the server's as it logs them, and a
/// verifier's as it replays a published log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRules(PlayersData);

impl SlotRules {
    pub(crate) const fn new(players: PlayersData) -> Self {
        Self(players)
    }

    /// What a leaver's slot becomes, as the mode's `leaver` says.
    pub(crate) const fn after_leave(self) -> AfterLeave {
        self.0.leaver
    }

    /// Whether the mode allows `change`: a player who takes back their own slot always; a new
    /// one an open slot with late join, a bot's with bot takeover; a leaver's slot only as
    /// `leaver` says.
    pub(crate) const fn check(self, change: SlotChangeKind) -> Result<(), SlotRuleError> {
        let PlayersData {
            late_join,
            bot_takeover,
            leaver,
        } = self.0;
        match change {
            SlotChangeKind::Joined { from: Taken::Open } if !late_join => {
                Err(SlotRuleError::LateJoin)
            }
            SlotChangeKind::Joined { from: Taken::Bot } if !bot_takeover => {
                Err(SlotRuleError::BotTakeover)
            }
            SlotChangeKind::Joined { .. } => Ok(()),
            SlotChangeKind::Left { becomes } => match (leaver, becomes) {
                (AfterLeave::Reserve, AfterLeave::Reserve)
                | (AfterLeave::Bot, AfterLeave::Bot)
                | (AfterLeave::Open, AfterLeave::Open) => Ok(()),
                _ => Err(SlotRuleError::Leaver { becomes }),
            },
        }
    }
}

#[cfg(test)]
mod tests;
