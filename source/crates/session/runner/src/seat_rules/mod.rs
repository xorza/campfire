use campfire_common::AfterLeave;
use campfire_mode::PlayersData;
use campfire_protocol::{SeatChangeKind, Taken};

use crate::seat_rules::error::SeatRuleError;

pub(crate) mod error;

/// The mode's rules of who may take a seat, as `[players]` gives them, which every change of a
/// seat's controller in a session of the mode keeps: the server's as it logs them, and a
/// verifier's as it replays a published log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeatRules(PlayersData);

impl SeatRules {
    pub(crate) const fn new(players: PlayersData) -> Self {
        Self(players)
    }

    /// What a leaver's seat becomes, as the mode's `leaver` says.
    pub(crate) const fn after_leave(self) -> AfterLeave {
        self.0.leaver
    }

    /// Whether the mode allows `change`: a player who takes back their own seat always; a new
    /// one an open seat with late join, a bot's with bot takeover; a leaver's seat only as
    /// `leaver` says.
    pub(crate) const fn check(self, change: SeatChangeKind) -> Result<(), SeatRuleError> {
        let PlayersData {
            late_join,
            bot_takeover,
            leaver,
            control: _,
        } = self.0;
        match change {
            SeatChangeKind::Joined { from: Taken::Open } if !late_join => {
                Err(SeatRuleError::LateJoin)
            }
            SeatChangeKind::Joined { from: Taken::Bot } if !bot_takeover => {
                Err(SeatRuleError::BotTakeover)
            }
            SeatChangeKind::Joined { .. } => Ok(()),
            SeatChangeKind::Left { becomes } => match (leaver, becomes) {
                (AfterLeave::Reserve, AfterLeave::Reserve)
                | (AfterLeave::Bot, AfterLeave::Bot)
                | (AfterLeave::Open, AfterLeave::Open) => Ok(()),
                _ => Err(SeatRuleError::Leaver { becomes }),
            },
        }
    }
}

#[cfg(test)]
mod tests;
