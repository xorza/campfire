use campfire_common::NamedEnum;

/// The effects the design names that the release does not run yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum PlannedEffect {
    Loot,
    Noise,
}
