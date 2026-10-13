use std::iter::FusedIterator;
use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::declare::scratch::Scratch;
use crate::stats::lifetime::Hold;
use crate::stats::modifiers::Modifiers;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::values::rank::Rank;

/// The grants the stage's sources want, one run for each unit a source visited, in the order the
/// sources pushed them, which `GrantPass` applies: each source keeps its own test of what changed,
/// and visits only those units. Not state: it empties within the stage.
#[derive(Resource, Debug, Default)]
pub(crate) struct Grants {
    grants: Vec<Grant>,
    runs: Vec<GrantRun>,
}

/// A modifier a source grants a unit from itself, held by `hold`, by `action` if one, at `rank`;
/// `None` for one the source granted and grants no more, which the pass releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grant {
    pub(crate) modifier: ModifierId,
    pub(crate) hold: Hold,
    pub(crate) action: Option<ActionId>,
    pub(crate) rank: Option<Rank>,
}

/// What a grant changes on its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantChange {
    /// The modifier applies at this rank.
    Apply(Rank),
    /// The modifier is released.
    Release,
}

/// The grants of one unit, where they start.
#[derive(Debug, Clone, Copy)]
struct GrantRun {
    entity: Entity,
    unit: StableId,
    start: u32,
}

/// The grants a source pushed for the unit `unit`, at `entity`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitGrants<'a> {
    pub(crate) entity: Entity,
    pub(crate) unit: StableId,
    pub(crate) grants: &'a [Grant],
}

/// The runs of `Grants`, in the order pushed.
#[derive(Debug)]
pub(crate) struct GrantsIter<'a> {
    grants: &'a Grants,
    at: Range<usize>,
}

impl Grant {
    /// What the grant changes on the unit `unit`, which carries `modifiers`: an apply at its rank
    /// when the unit does not hold the modifier from itself at that rank by the grant's hold, a
    /// release when it holds it and the grant has no rank, and nothing else.
    pub(crate) fn change(self, modifiers: &Modifiers, unit: StableId) -> Option<GrantChange> {
        let held = modifiers
            .get(self.modifier, Some(unit))
            .filter(|instance| instance.lifetime.held_by(self.hold))
            .map(|instance| instance.rank);
        match self.rank {
            Some(rank) if held != Some(rank) => Some(GrantChange::Apply(rank)),
            None if held.is_some() => Some(GrantChange::Release),
            Some(_) | None => None,
        }
    }
}

impl Grants {
    /// Starts the run of the unit `unit`, at `entity`: the grants pushed until the next run are
    /// its.
    pub(crate) fn unit(&mut self, entity: Entity, unit: StableId) {
        let start = u32::try_from(self.grants.len()).expect("a stage's grants fit u32");
        self.runs.push(GrantRun {
            entity,
            unit,
            start,
        });
    }

    /// Adds `grant` to the run started last.
    pub(crate) fn push(&mut self, grant: Grant) {
        debug_assert!(!self.runs.is_empty(), "a grant is pushed in a unit's run");
        self.grants.push(grant);
    }

    pub(crate) fn clear(&mut self) {
        self.grants.clear();
        self.runs.clear();
    }

    pub(crate) const fn iter(&self) -> GrantsIter<'_> {
        GrantsIter {
            grants: self,
            at: 0..self.runs.len(),
        }
    }

    fn run(&self, at: usize) -> UnitGrants<'_> {
        let GrantRun {
            entity,
            unit,
            start,
        } = self.runs[at];
        let end = self
            .runs
            .get(at + 1)
            .map_or(self.grants.len(), |next| next.start as usize);
        UnitGrants {
            entity,
            unit,
            grants: &self.grants[start as usize..end],
        }
    }
}

impl<'a> IntoIterator for &'a Grants {
    type Item = UnitGrants<'a>;
    type IntoIter = GrantsIter<'a>;

    fn into_iter(self) -> GrantsIter<'a> {
        self.iter()
    }
}

impl<'a> Iterator for GrantsIter<'a> {
    type Item = UnitGrants<'a>;

    fn next(&mut self) -> Option<UnitGrants<'a>> {
        self.at.next().map(|at| self.grants.run(at))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.at.size_hint()
    }
}

impl ExactSizeIterator for GrantsIter<'_> {}

impl FusedIterator for GrantsIter<'_> {}

impl Scratch for Grants {
    fn is_clear(&self) -> bool {
        self.runs.is_empty()
    }
}
