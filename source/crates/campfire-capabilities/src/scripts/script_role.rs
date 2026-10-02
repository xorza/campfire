/// What a script serves, as the data that names it says: each role has its own hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptRole {
    Action,
    Modifier,
    Mode,
    Ai,
}

impl ScriptRole {
    pub const ALL: [ScriptRole; 4] = [
        ScriptRole::Action,
        ScriptRole::Modifier,
        ScriptRole::Mode,
        ScriptRole::Ai,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ScriptRole::Action => "action",
            ScriptRole::Modifier => "modifier",
            ScriptRole::Mode => "mode",
            ScriptRole::Ai => "AI",
        }
    }
}
