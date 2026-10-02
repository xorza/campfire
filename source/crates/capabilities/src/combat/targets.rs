use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};

use crate::combat::combat_bindings::CombatBindings;
use crate::combat::dead::Dead;
use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::living_unit::LivingUnit;
use crate::units::relations::Relations;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::values::attitude::Attitude;
use crate::values::metric::Metric;

/// The units an attack may target: living units with the life pool whose tags let them be
/// targets, those a client holds and does not predict among them, where the server last had
/// them. Every capability that chooses or checks a target goes through it, so all agree on what a
/// valid target is.
#[derive(SystemParam, Debug)]
pub(crate) struct Targets<'w, 's> {
    index: Res<'w, EntityIndex>,
    relations: Res<'w, Relations>,
    bindings: Res<'w, CombatBindings>,
    metric: Res<'w, Metric>,
    units: Query<
        'w,
        's,
        (
            &'static StableId,
            &'static Position,
            &'static Team,
            &'static Pools,
            Option<&'static Body>,
            Option<&'static UnitTags>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
}

impl Targets<'_, '_> {
    /// `target`, when it is a living unit `team` may attack.
    pub(crate) fn enemy(&self, team: Team, target: StableId) -> Option<LivingUnit> {
        self.living(target)
            .filter(|unit| self.attitude(team, unit.team).may_attack())
    }

    pub(crate) fn metric(&self) -> Metric {
        *self.metric
    }

    /// How `of` regards `other`.
    pub(crate) fn attitude(&self, of: Team, other: Team) -> Attitude {
        self.relations.between(of, other)
    }

    /// Whether `range` from a unit at `from` of body radius `radius` reaches `target`: within
    /// range in the map's metric, exactly, from the edge of the one body to the edge of the other.
    pub(crate) fn reaches(
        &self,
        from: Position,
        radius: Num,
        range: Num,
        target: &LivingUnit,
    ) -> bool {
        self.metric
            .within(from, target.pos, range + radius + target.radius)
    }

    /// Whether `range` from a unit at `from` of body radius `radius` reaches the point `at`, from
    /// the edge of its body, exactly.
    pub(crate) fn reaches_point(
        &self,
        from: Position,
        radius: Num,
        range: Num,
        at: Position,
    ) -> bool {
        self.metric.within(from, at, range + radius)
    }

    /// Every living unit that may be a target, in no order.
    pub(crate) fn units(&self) -> impl Iterator<Item = LivingUnit> + '_ {
        self.units.iter().filter_map(|(&id, ..)| self.living(id))
    }

    /// `target`, when it is a living unit that may be a target.
    pub(crate) fn living(&self, target: StableId) -> Option<LivingUnit> {
        let row = self.units.get(self.index.get(target)?).ok()?;
        let blocked = UnitTags::effects_of(row.5).blocks(Block::Target);
        self.body(row).filter(|_| !blocked)
    }

    /// Every living unit with the life pool, those whose tags block it as a target among them:
    /// the units an area reaches, in no order.
    pub(crate) fn bodies(&self) -> impl Iterator<Item = LivingUnit> + '_ {
        self.units.iter().filter_map(|row| self.body(row))
    }

    /// The unit of `row`, when it has the life pool.
    fn body(
        &self,
        (&id, &pos, &team, pools, body, tags): (
            &StableId,
            &Position,
            &Team,
            &Pools,
            Option<&Body>,
            Option<&UnitTags>,
        ),
    ) -> Option<LivingUnit> {
        pools.max(self.bindings.life)?;
        Some(LivingUnit {
            id,
            pos,
            team,
            radius: Body::radius_of(body),
            tags: tags.map_or(TagSet::default(), |tags| tags.tags),
        })
    }
}
