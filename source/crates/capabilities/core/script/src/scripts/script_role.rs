use crate::scripts::error::ApiError;
use crate::scripts::name_kind::NameKind;

campfire_common::enum_all! {
    /// What a script serves, as the data that names it says: each role has its own hooks.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum ScriptRole {
        Action,
        Modifier,
        Mode,
        Ai,
    }
    pub const ALL;
}

impl ScriptRole {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Modifier => "modifier",
            Self::Mode => "mode",
            Self::Ai => "AI",
        }
    }
}

campfire_common::enum_all! {
    /// Whose params a call reads as `ctx.p`, by its role: an ability's and its modifier's, or the
    /// mode's.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ParamScope {
        Action,
        Mode,
    }
    pub const ALL;
}

impl ScriptRole {
    /// The scope of the params a call of the role reads: an action's or a modifier's call reads
    /// its ability's and modifier's, a mode's or an AI's the mode's.
    pub const fn params(self) -> ParamScope {
        match self {
            Self::Action | Self::Modifier => ParamScope::Action,
            Self::Mode | Self::Ai => ParamScope::Mode,
        }
    }
}

impl ParamScope {
    /// Its place among `ALL`.
    pub const fn index(self) -> usize {
        match self {
            Self::Action => 0,
            Self::Mode => 1,
        }
    }

    /// The failure of a call that reads a param of the scope where no part holds them: the mode's
    /// in a match with no mode, an ability's in a match with no stats.
    pub(crate) const fn missing(self) -> ApiError {
        match self {
            Self::Action => ApiError::Unknown(NameKind::Param),
            Self::Mode => ApiError::NoMode,
        }
    }
}
