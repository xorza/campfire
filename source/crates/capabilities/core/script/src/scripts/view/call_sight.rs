use campfire_values::PlayerView;

use crate::scripts::call_start::CallStart;

/// Who reads the view in the running call (06-script-api.md, What a team sees is one rule of the
/// view): the match, as every role of the sim reads it, whole; or a player, as a bot's think reads
/// it, which sees only the units its team sees, and of each only what its player slot's client
/// receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallSight {
    Match,
    Player(PlayerView),
}

impl CallSight {
    /// Who reads the view in the call `start`: a bot's call reads as the player it plays as.
    pub(crate) const fn of(start: &CallStart) -> Self {
        match start.sight {
            Some(player) => Self::Player(player),
            None => Self::Match,
        }
    }
}
