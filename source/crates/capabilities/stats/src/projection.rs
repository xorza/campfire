use campfire_math::{Num, Vec3};
use campfire_names::Filter;
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank, Relation, TagSet, Team};

/// A modifier held this tick on each living unit whose body comes within `radius` of `at` in the
/// map's metric, that `affects` selects as `team` regards it, from `source`, by its `action` at
/// its rank: an aura's and an area's, which stats' held pass selects by one rule
/// (04-capabilities/stats.md, An area holds as an aura does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Projection {
    pub at: Vec3,
    pub radius: Num,
    pub team: Team,
    pub affects: Affects,
    pub modifier: ModifierId,
    pub source: StableId,
    pub action: Option<ActionId>,
    pub rank: Rank,
}

/// The units a projection selects among those it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affects {
    /// Those the filter selects.
    Filter(Filter),
    /// Its source alone.
    Source,
    /// Those the filter selects, but its source.
    Others(Filter),
}

impl Projection {
    /// Whether it selects `unit`, with `tags`, of a team its team regards with `relation`.
    pub(crate) fn selects(&self, unit: StableId, relation: Relation, tags: TagSet) -> bool {
        match self.affects {
            Affects::Filter(filter) => filter.selects(relation, tags),
            Affects::Source => unit == self.source,
            Affects::Others(filter) => unit != self.source && filter.selects(relation, tags),
        }
    }
}
