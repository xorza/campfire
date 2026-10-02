use crate::units::tag::Tag;

/// A tag the engine gives and reads, at the same place in every match's tags: packages name it
/// in filters and in the mode's `[tags]`, but no unit type or modifier of theirs carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineTag {
    /// Of avatars, which `unit.is_avatar` tests.
    Avatar,
    /// Of projectile types: a filter selects their units only when it names it.
    Projectile,
    /// Of area types: a filter selects their units only when it names it.
    Area,
}

impl EngineTag {
    /// In the order of their places, the first of the match's tags.
    pub const ALL: [EngineTag; 3] = [EngineTag::Avatar, EngineTag::Projectile, EngineTag::Area];

    pub const fn name(self) -> &'static str {
        match self {
            EngineTag::Avatar => "avatar",
            EngineTag::Projectile => "projectile",
            EngineTag::Area => "area",
        }
    }

    /// The engine tag `name`, if it is one.
    pub fn named(name: &str) -> Option<EngineTag> {
        EngineTag::ALL.into_iter().find(|tag| tag.name() == name)
    }

    pub(crate) fn tag(self) -> Tag {
        Tag::new(self as usize)
    }
}
