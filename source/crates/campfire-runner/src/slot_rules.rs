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
mod tests {
    use super::*;

    #[test]
    fn the_rules_allow_each_change_the_modes_players_allow() {
        let rules = |late_join, bot_takeover, leaver| {
            SlotRules::new(PlayersData {
                late_join,
                bot_takeover,
                leaver,
            })
        };
        let joined = |from| SlotChangeKind::Joined { from };
        let left = |becomes| SlotChangeKind::Left { becomes };
        let closed = rules(false, false, Leaver::Reserve);
        assert_eq!(closed.check(joined(Taken::Own)), Ok(()));
        assert_eq!(
            closed.check(joined(Taken::Open)),
            Err(SlotRuleError::LateJoin)
        );
        assert_eq!(
            closed.check(joined(Taken::Bot)),
            Err(SlotRuleError::BotTakeover)
        );
        assert_eq!(closed.check(left(AfterLeave::Reserve)), Ok(()));
        let open = rules(true, true, Leaver::Bot);
        assert_eq!(open.check(joined(Taken::Open)), Ok(()));
        assert_eq!(open.check(joined(Taken::Bot)), Ok(()));
        assert_eq!(open.check(left(AfterLeave::Bot)), Ok(()));
        for becomes in [AfterLeave::Reserve, AfterLeave::Open] {
            assert_eq!(
                open.check(left(becomes)),
                Err(SlotRuleError::Leaver { becomes })
            );
        }
        // Late join alone takes open slots, no bot's.
        let late = rules(true, false, Leaver::Open);
        assert_eq!(late.check(joined(Taken::Open)), Ok(()));
        assert_eq!(
            late.check(joined(Taken::Bot)),
            Err(SlotRuleError::BotTakeover)
        );
        assert_eq!(late.check(left(AfterLeave::Open)), Ok(()));
        // What a leaver's slot becomes, by each `leaver`.
        let becomes = [Leaver::Reserve, Leaver::Bot, Leaver::Open]
            .map(|leaver| rules(true, true, leaver).after_leave());
        assert_eq!(
            becomes,
            [AfterLeave::Reserve, AfterLeave::Bot, AfterLeave::Open]
        );
    }
}
