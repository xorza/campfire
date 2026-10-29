use campfire_common::EnumAll;
use campfire_values::{Named, Param};

use crate::scripts::error::ApiError;

/// What a script serves, as the data that names it says: each role has its own hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum ScriptRole {
    Action,
    Modifier,
    Mode,
    Ai,
    /// A bot's, which the server's bot driver calls outside the sim (03-game-scripting.md, A bot
    /// is a Rhai script of its game, which the server's bot driver runs).
    Bot,
}

impl ScriptRole {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Modifier => "modifier",
            Self::Mode => "mode",
            Self::Ai => "AI",
            Self::Bot => "bot",
        }
    }
}

/// Whose params a call reads as `ctx.params`, by its role: an ability's and its modifier's, the
/// mode's, or a bot's then the mode's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ParamScope {
    Action,
    Mode,
    Bot,
}

impl ScriptRole {
    /// The scope of the params a call of the role reads: an action's or a modifier's call reads
    /// its ability's and modifier's, a mode's or an AI's the mode's, a bot's its own then the
    /// mode's.
    pub const fn params(self) -> ParamScope {
        match self {
            Self::Action | Self::Modifier => ParamScope::Action,
            Self::Mode | Self::Ai => ParamScope::Mode,
            Self::Bot => ParamScope::Bot,
        }
    }
}

impl ParamScope {
    /// The scope a param the scope lacks is read from: a bot's lie over the mode's.
    pub const fn falls_back(self) -> Option<Self> {
        match self {
            Self::Bot => Some(Self::Mode),
            Self::Action | Self::Mode => None,
        }
    }

    /// Its place among `ALL`.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The failure of a call that reads a param of the scope where no part holds them: the mode's
    /// in a match with no mode, an ability's in a match with no stats.
    pub(crate) const fn missing(self) -> ApiError {
        match self {
            Self::Action | Self::Bot => ApiError::Unknown(Param::NAMES),
            Self::Mode => ApiError::NoMode,
        }
    }
}
