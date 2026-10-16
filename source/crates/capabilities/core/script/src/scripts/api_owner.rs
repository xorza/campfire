use campfire_common::EnumAll;

/// What a script holds a name on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll)]
pub enum ApiOwner {
    Ctx,
    Unit,
    NewUnit,
    Modifier,
    Hit,
    Damage,
    Heal,
    Vector,
    GameMap,
    Marker,
}

impl ApiOwner {
    /// The owner as the reference titles it.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Ctx => "`ctx`",
            Self::Unit => "Unit",
            Self::NewUnit => "New unit, of `spawn_unit`, `ctx.projectile` and `ctx.area`",
            Self::Modifier => "Modifier `m`",
            Self::Hit => "Hit `hit`",
            Self::Damage => "Damage `d`",
            Self::Heal => "Heal `h`",
            Self::Vector => "Vector",
            Self::GameMap => "Map, `ctx.map`",
            Self::Marker => "Marker, of `ctx.map.markers(tag)`",
        }
    }
}
