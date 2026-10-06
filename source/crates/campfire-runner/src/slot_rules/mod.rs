use campfire_capabilities::{Leaver, PlayersData};
use campfire_protocol::{AfterLeave, SlotChangeKind, Taken};

use crate::error::SlotRuleError;

/// The mode's rules of who may take a slot, as `[players]` gives them, which every change of a
/// slot's controller in a session of the mode keeps: the server's as it logs them, and a
/// verifier's as it replays a published log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRules(PlayersData);

impl SlotRules {
    pub(crate) const fn new(players: PlayersData) -> SlotRules {
        SlotRules(players)
    }

    /// What a leaver's slot becomes, as the mode's `leaver` says.
    pub(crate) const fn after_leave(self) -> AfterLeave {
        match self.0.leaver {
            Leaver::Reserve => AfterLeave::Reserve,
            Leaver::Bot => AfterLeave::Bot,
            Leaver::Open => AfterLeave::Open,
        }
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
                (Leaver::Reserve, AfterLeave::Reserve)
                | (Leaver::Bot, AfterLeave::Bot)
                | (Leaver::Open, AfterLeave::Open) => Ok(()),
                _ => Err(SlotRuleError::Leaver { becomes }),
            },
        }
    }
}

#[cfg(test)]
mod tests;
