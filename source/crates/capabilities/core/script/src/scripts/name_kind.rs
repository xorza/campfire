use derive_more::Display;

/// What a name in data or in a script names, as the load checks it: a call's argument the
/// registry marks as a name of this kind, or a name a data file gives.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameKind {
    #[display("param")]
    Param,
    #[display("modifier")]
    Modifier,
    #[display("stat")]
    Stat,
    #[display("pool")]
    Pool,
    /// A pool or a player resource, which a cost takes from.
    #[display("pool or player resource")]
    Cost,
    /// A tag of the map's markers.
    #[display("marker with tag")]
    MarkerTag,
    #[display("player resource")]
    Resource,
    #[display("layer")]
    Layer,
    #[display("filter")]
    Filter,
    #[display("damage kind")]
    DamageKind,
    #[display("track")]
    Track,
    #[display("unit type")]
    UnitType,
    #[display("message")]
    Message,
    /// A tag of the match's units.
    #[display("tag")]
    Tag,
    #[display("team")]
    Team,
    #[display("path")]
    Path,
    #[display("choice")]
    Choice,
    #[display("slot kind")]
    SlotKind,
    /// An action of the script's own package.
    #[display("ability")]
    Ability,
    /// An item type of the mode package.
    #[display("item type")]
    Item,
}
