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
/// receives its entities, and whether its owner's client predicts one.
pub trait EntityKind: KindMarker {
    const AUDIENCE: KindAudience;
    const OWNER_PREDICTS: bool;
}
