use std::fmt;

/// What a name in data or in a script names, as the load checks it: a call's argument the
/// registry marks as a name of this kind, or a name a data file gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameKind {
    Param,
    Modifier,
    Stat,
    Pool,
    /// A pool or a player resource, which a cost takes from.
    Cost,
    /// A tag of the map's markers.
    MarkerTag,
    Resource,
    Layer,
    Filter,
    DamageKind,
    Track,
    UnitType,
    Message,
    /// A tag of the match's units.
    Tag,
    Team,
    Path,
    Choice,
    SlotKind,
    /// An action of the script's own package.
    Ability,
    /// An item type of the mode package.
    Item,
}

impl fmt::Display for NameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NameKind::Param => "param",
            NameKind::Modifier => "modifier",
            NameKind::Stat => "stat",
            NameKind::Pool => "pool",
            NameKind::Cost => "pool or player resource",
            NameKind::MarkerTag => "marker with tag",
            NameKind::Resource => "player resource",
            NameKind::Layer => "layer",
            NameKind::Filter => "filter",
            NameKind::DamageKind => "damage kind",
            NameKind::Track => "track",
            NameKind::UnitType => "unit type",
            NameKind::Message => "message",
            NameKind::Tag => "tag",
            NameKind::Team => "team",
            NameKind::Path => "path",
            NameKind::Choice => "choice",
            NameKind::SlotKind => "slot kind",
            NameKind::Item => "item type",
            NameKind::Ability => "ability",
        })
    }
}
