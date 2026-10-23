use derive_more::Display;

/// A fixed set the engine owns, which scripts hold as members of an enum, `Relation::Hostile`,
/// not as strings, (06-script-api.md, The engine's sets are enums, a mode's names strings): the
/// enum's name as scripts write it, the module of its members. The crate of each enum makes its
/// own, so the core names no capability's.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[display("{name}")]
pub struct EngineEnum {
    name: &'static str,
}

impl EngineEnum {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// The enum as scripts name it, the module of its members.
    pub const fn name(self) -> &'static str {
        self.name
    }
}
