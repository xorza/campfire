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

/// What a grant changes on its unit (04-capabilities/stats.md, The holds of an instance agree on
/// its rank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantChange {
    /// The modifier applies at this rank, as the unit holds no instance of it from the source.
    Apply(Rank),
    /// The instance takes this rank: its shares renew at it, and its stacks stay.
    Rank(Rank),
    /// The hold joins the instance, which changes nothing else.
    Join,
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

    /// What the grant changes on the unit `unit`, which carries `modifiers`. With no instance from
    /// the grant's source, an apply at its rank. With one, the holds agree on the highest rank
    /// any asks: a grant above it raises it, one below it lowers it only by a hold that asks a
    /// rank of its own and is the only such hold, and any other joins it, if its hold does not
    /// already; a grant of no rank releases its hold.
    pub fn change(self, modifiers: &Modifiers, unit: StableId) -> Option<GrantChange> {
        let Some(carried) = modifiers.get(self.modifier, self.source(unit)) else {
            return self.rank.map(GrantChange::Apply);
        };
        let instance = carried.instance;
        let held = instance.lifetime.held_by(self.hold);
        let Some(rank) = self.rank else {
            return held.then_some(GrantChange::Release);
        };
        let lowers = held
            && rank < instance.rank
            && self.hold.asks_rank()
            && !instance.lifetime.ranked_besides(self.hold);
        if rank > instance.rank || lowers {
            Some(GrantChange::Rank(rank))
        } else {
            (!held).then_some(GrantChange::Join)
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
    /// Pushes the grants that make the unit hold exactly `wanted`, each from `from` by `hold` at
    /// rank 1, of what it holds now, `modifiers`: an apply of each wanted one `hold` does not hold,
    /// and a release of each it holds and does not want. Every source of a hold of rank 1 diffs
    /// alike, so a unit whose holds stand opens no run; one a hold of a higher rank raised stays
    /// held (04-capabilities/stats.md, Passives by one pass).
    pub fn hold_exactly(
        &mut self,
        hold: Hold,
        from: GrantFrom,
        wanted: &[ModifierId],
        modifiers: Option<&Modifiers>,
    ) {
        let source = match from {
            GrantFrom::Itself => Some(self.unit),
            GrantFrom::Nobody => None,
        };
        let held = modifiers
            .into_iter()
            .flat_map(Modifiers::iter)
            .filter(|carried| {
                let instance = carried.instance;
                instance.source == source && instance.lifetime.held_by(hold)
            });
        let grant = |modifier, rank| Grant {
            modifier,
            from,
            hold,
            action: None,
            rank,
        };
        for carried in held.clone() {
            let instance = carried.instance;
            if !wanted.contains(&instance.id) {
                self.push(grant(instance.id, None));
            }
        }
        for &modifier in wanted {
            if !held.clone().any(|carried| carried.instance.id == modifier) {
                self.push(grant(modifier, Some(Rank::FIRST)));
            }
        }
    }

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
