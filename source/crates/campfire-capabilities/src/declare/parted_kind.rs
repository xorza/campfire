use campfire_sim::SimComponent;

/// A kind of entity whose one spawn puts on the parts the modules declare for it, each made from
/// what the world holds then: the match's entity, and each player's. A unit's parts come from its
/// kit instead.
pub(crate) trait PartedKind: SimComponent {}
