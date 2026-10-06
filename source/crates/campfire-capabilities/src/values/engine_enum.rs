use derive_more::Display;

/// A fixed set the engine owns, which scripts hold as members of an enum, `Relation::Hostile`,
/// not as strings, as design 08's Engine enums gives them.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[display("{}", self.name())]
pub enum EngineEnum {
    /// How one team regards another, as `set_relation` takes it.
    Relation,
    /// The end of a path a spawn group walks it from, as `spawn_group` takes it.
    PathEnd,
}

impl EngineEnum {
    /// The enum as scripts name it, the module of its members.
    pub const fn name(self) -> &'static str {
        match self {
            EngineEnum::Relation => "Relation",
            EngineEnum::PathEnd => "PathEnd",
        }
    }
}
