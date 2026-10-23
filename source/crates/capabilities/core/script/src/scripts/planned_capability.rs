use campfire_common::NamedEnum;

/// A capability the design plans and the release does not build, the home of names of the
/// script API that are planned with it: the design's alone, no capability of the engine
/// (04-capabilities/00-overview.md, A planned capability is the design's alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum PlannedCapability {
    Character,
    Hitboxes,
    Physics,
    World,
    Quests,
}
