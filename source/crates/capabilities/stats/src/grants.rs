use std::iter::FusedIterator;
use std::ops::Range;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_declarations::{Role, Scratch};
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank};

use crate::lifetime::Hold;
use crate::modifiers::Modifiers;

/// The grants the stage's sources want, one run for each unit a source visited, in the order the
/// sources pushed them, which `GrantPass` applies: each source keeps its own test of what changed,
/// and visits only those units. Not state: it empties within the stage.
#[derive(Resource, Debug, Default)]
pub struct Grants {
    grants: Vec<Grant>,
    runs: Vec<GrantRun>,
}

/// A modifier a source grants a unit, from `from`, held by `hold`, by `action` if one, at
/// `rank`; `None` for one the source granted and grants no more, which the pass releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub modifier: ModifierId,
    pub from: GrantFrom,
    pub hold: Hold,
    pub action: Option<ActionId>,
    pub rank: Option<Rank>,
}

/// Whom a grant's instance is from: the unit itself, as its passives and items, or no source,
/// as its player's modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantFrom {
    Itself,
    Nobody,
}

/// What a grant changes on its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantChange {
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
pub struct UnitGrants<'a> {
    pub(crate) entity: Entity,
    pub(crate) unit: StableId,
    pub(crate) grants: &'a [Grant],
}

/// The run of the unit `unit`, at `entity`, as a source pushes its grants: it opens at its first
/// grant, so a unit a source visits and grants nothing costs the pass nothing.
#[derive(Debug)]
pub struct UnitRun<'a> {
    grants: &'a mut Grants,
    entity: Entity,
    unit: StableId,
    opened: bool,
}

/// The runs of `Grants`, in the order pushed.
#[derive(Debug)]
pub struct GrantsIter<'a> {
    grants: &'a Grants,
    at: Range<usize>,
}

impl Grant {
    /// The source of the grant's instance on the unit `unit`.
    pub const fn source(self, unit: StableId) -> Option<StableId> {
        match self.from {
            GrantFrom::Itself => Some(unit),
            GrantFrom::Nobody => None,
        }
    }

    /// What the grant changes on the unit `unit`, which carries `modifiers`: an apply at its rank
    /// when the unit does not hold the modifier from the grant's source at that rank by the
    /// grant's hold, a release when it holds it and the grant has no rank, and nothing else.
    pub fn change(self, modifiers: &Modifiers, unit: StableId) -> Option<GrantChange> {
        let held = modifiers
            .get(self.modifier, self.source(unit))
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
    /// The run of the unit `unit`, at `entity`, to push its grants into.
    pub const fn of(&mut self, entity: Entity, unit: StableId) -> UnitRun<'_> {
        UnitRun {
            grants: self,
            entity,
            unit,
            opened: false,
        }
    }

    pub fn clear(&mut self) {
        self.grants.clear();
        self.runs.clear();
    }

    pub const fn iter(&self) -> GrantsIter<'_> {
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

impl UnitRun<'_> {
    /// Adds `grant` to the unit's run, which it opens first.
    pub fn push(&mut self, grant: Grant) {
        let grants = &mut *self.grants;
        if !self.opened {
            let start = u32::try_from(grants.grants.len()).expect("a stage's grants fit u32");
            grants.runs.push(GrantRun {
                entity: self.entity,
                unit: self.unit,
                start,
            });
            self.opened = true;
        }
        grants.grants.push(grant);
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
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_run_opens_at_its_first_grant() {
        let mut ids = IdAllocator::default();
        let [quiet, granted] = [(); 2].map(|()| ids.allocate());
        let grant = |rank| Grant {
            modifier: ModifierId::new(0),
            from: GrantFrom::Itself,
            hold: Hold::Passive,
            action: None,
            rank,
        };
        // A unit visited and granted nothing has no run; one granted twice has one run of both.
        let mut grants = Grants::default();
        grants.of(Entity::from_raw_u32(1).unwrap(), quiet);
        let mut run = grants.of(Entity::from_raw_u32(2).unwrap(), granted);
        run.push(grant(Some(Rank::FIRST)));
        run.push(grant(None));
        let runs: Vec<_> = grants
            .iter()
            .map(|run| (run.unit, run.grants.len()))
            .collect();
        assert_eq!(runs, [(granted, 2)]);
        assert_eq!(grants.iter().len(), 1);
    }
}
