use campfire_common::EnumAll;

/// The effects the design names that the release does not run yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum PlannedEffect {
    Loot,
    Noise,
}

impl PlannedEffect {
    /// The effect's key in data.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Loot => "loot",
            Self::Noise => "noise",
        }
    }

    /// The effect of the key `name`, if one is planned.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|effect| effect.name() == name)
    }
}
