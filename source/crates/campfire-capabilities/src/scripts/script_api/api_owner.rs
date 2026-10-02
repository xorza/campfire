/// What a script holds a name on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiOwner {
    Ctx,
    Unit,
    NewUnit,
    Modifier,
    Hit,
    Damage,
    Heal,
    Position,
    Vector,
    GameMap,
    Marker,
}
impl ApiOwner {
    pub const ALL: [ApiOwner; 11] = [
        ApiOwner::Ctx,
        ApiOwner::Unit,
        ApiOwner::NewUnit,
        ApiOwner::Modifier,
        ApiOwner::Hit,
        ApiOwner::Damage,
        ApiOwner::Heal,
        ApiOwner::Position,
        ApiOwner::Vector,
        ApiOwner::GameMap,
        ApiOwner::Marker,
    ];

    /// The owner as the reference titles it.
    pub const fn title(self) -> &'static str {
        match self {
            ApiOwner::Ctx => "`ctx`",
            ApiOwner::Unit => "Unit",
            ApiOwner::NewUnit => "New unit, of `spawn_unit`, `ctx.projectile` and `ctx.area`",
            ApiOwner::Modifier => "Modifier `m`",
            ApiOwner::Hit => "Hit `hit`",
            ApiOwner::Damage => "Damage `d`",
            ApiOwner::Heal => "Heal `h`",
            ApiOwner::Position => "Position",
            ApiOwner::Vector => "Vector",
            ApiOwner::GameMap => "Map, `ctx.map`",
            ApiOwner::Marker => "Marker, of `ctx.map.markers(tag)`",
        }
    }
}
