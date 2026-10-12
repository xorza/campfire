use campfire_sim::KindMarker;

/// Who receives the entities of a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindAudience {
    /// The links of the teams that see an entity, as vision says.
    Seers,
    /// Every seated link, whatever vision says.
    EverySeat,
}

/// A kind of entity, by its marker, which each entity of the kind holds and no other: who
/// receives its entities, whether its owner's client predicts one, and whether its one spawn puts
/// on the parts the modules declare for it.
pub trait EntityKind: KindMarker {
    const AUDIENCE: KindAudience;
    const OWNER_PREDICTS: bool;
    /// Whether an entity of the kind spawns with the parts the modules declare for it, each made
    /// from what the world holds then: the match's entity, and each player's. A unit's parts come
    /// from its kit instead.
    const DECLARED_PARTS: bool;
}
