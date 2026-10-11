use bevy_ecs::component::Component;
use bevy_ecs::query::QueryFilter;
use campfire_sim::HostRule;

/// Who receives the entities of a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindAudience {
    /// The links of the teams that see an entity, as vision says.
    Seers,
    /// Every seated link, whatever vision says.
    EverySeat,
}

/// A kind of entity, by its marker: which state types sit on it, as a `HostRule` says, who
/// receives its entities, and whether its owner's client predicts one. An entity is of the
/// narrowest kind its markers name: a delivery is a unit, and of its own kind.
pub trait EntityKind: Component + HostRule {
    const AUDIENCE: KindAudience;
    const OWNER_PREDICTS: bool;
    /// The entities of this kind alone, a narrower kind's left out.
    type Only: QueryFilter + 'static;
}
