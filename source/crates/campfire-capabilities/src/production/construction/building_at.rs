use campfire_sim::StableId;

/// A builder that builds a site this tick, as the Mode stage finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BuildingAt {
    pub(super) site: StableId,
    pub(super) builder: StableId,
}
