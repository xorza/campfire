use campfire_common::EnumAll;

/// A capability the design plans and the release does not build, the home of names of the
/// script API that are planned with it: the design's alone, no capability of the engine
/// (04-capabilities/00-overview.md, A planned capability is the design's alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum PlannedCapability {
    Character,
    Hitboxes,
    Physics,
    World,
    Quests,
}

impl PlannedCapability {
    /// Its name, as the design's table of capabilities gives it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Hitboxes => "hitboxes",
            Self::Physics => "physics",
            Self::World => "world",
            Self::Quests => "quests",
        }
    }
}
