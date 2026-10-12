use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};

use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::players::on_match::OnMatch;
use crate::stats::life::Life;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::body_grid::Placed;
use crate::units::dead::Dead;
use crate::units::living_unit::LivingUnit;
use crate::units::relations::Relations;
use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::values::relation::Relation;

/// The units an attack may target: living units with a life whose tags let them be
/// targets, those a client holds and does not predict among them, where the server last had
/// them. Every capability that chooses or checks a target goes through it, so all agree on what a
/// valid target is.
#[derive(SystemParam, Debug)]
pub(crate) struct Targets<'w, 's> {
    index: Res<'w, EntityIndex>,
    relations: OnMatch<'w, 's, Relations>,
    metric: Res<'w, Metric>,
    units: Query<
        'w,
        's,
        (
            &'static StableId,
            &'static Position,
            &'static Team,
            &'static Life,
            Option<&'static Body>,
            Option<&'static UnitTags>,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
}

/// What a body of the grid of `Targets::placed` holds of its unit: its team, its tags, and
/// whether they let it be a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetKey {
    pub(crate) team: Team,
    pub(crate) tags: TagSet,
    pub(crate) targetable: bool,
}

/// A row of the units `Targets` holds.
type TargetRow<'a> = (
    &'a StableId,
    &'a Position,
    &'a Team,
    &'a Life,
    Option<&'a Body>,
    Option<&'a UnitTags>,
);

impl Targets<'_, '_> {
    /// `target`, when it is a living unit `team` may attack.
    pub(crate) fn enemy(&self, team: Team, target: StableId) -> Option<LivingUnit> {
        self.living(target)
            .filter(|unit| self.relation(team, unit.team).may_attack())
    }

    pub(crate) fn metric(&self) -> Metric {
        *self.metric
    }

    /// How `of` regards `other`.
    pub(crate) fn relation(&self, of: Team, other: Team) -> Relation {
        self.relations.get().between(of, other)
    }

    /// Whether `range` from a unit at `from` of body `shape` reaches `target`: within range in
    /// the map's metric, exactly, from the edge of the one body to the edge of the other.
    pub(crate) fn reaches(
        &self,
        from: Position,
        shape: Shape,
        range: Num,
        target: &LivingUnit,
    ) -> bool {
        self.metric
            .reaches(from, shape, range, target.pos, target.shape)
    }

    /// Whether `range` from a unit at `from` of body `shape` reaches the point `pos`, from the
    /// edge of its body, exactly.
    pub(crate) fn reaches_point(
        &self,
        from: Position,
        shape: Shape,
        range: Num,
        pos: Position,
    ) -> bool {
        self.metric.reaches(from, shape, range, pos, Shape::POINT)
    }

    /// The point `step` from `from` toward `to` in the map's metric, or `to` when it is nearer.
    pub(crate) fn toward(&self, from: Position, to: Position, step: Num) -> Position {
        self.metric.step_toward(from, to, step)
    }

    /// `target`, when it is a living unit that may be a target.
    pub(crate) fn living(&self, target: StableId) -> Option<LivingUnit> {
        let row = self.units.get(self.index.get(target)?).ok()?;
        let (_, _, _, life, _, tags) = row;
        Targets::targetable(Some(life), tags).then(|| Targets::body(row))
    }

    /// Whether a unit that has not died, with `life` and `tags`, may be a target: it has a life,
    /// and its tags let it be one. This is the one rule of a target, which the script view reads
    /// too.
    pub(crate) fn targetable(life: Option<&Life>, tags: Option<&UnitTags>) -> bool {
        life.is_some() && !UnitTags::properties_of(tags).blocks(Block::Target)
    }

    /// Every living unit with the life pool, those whose tags block it as a target among them,
    /// as a body to index, with what a visit needs of it: the units an area reaches, in no order.
    pub(crate) fn placed(&self) -> impl Iterator<Item = Placed<TargetKey>> + '_ {
        self.units
            .iter()
            .map(|(&id, &at, &team, life, body, tags)| Placed {
                id,
                key: TargetKey {
                    team,
                    tags: tags.map_or(TagSet::default(), |tags| tags.tags),
                    targetable: Targets::targetable(Some(life), tags),
                },
                pos: at,
                shape: Body::shape_of(body),
            })
    }

    /// The unit of `body`, a body of the grid `placed` built, as it was when the grid was built.
    pub(crate) const fn unit_of(body: &Placed<TargetKey>) -> LivingUnit {
        LivingUnit {
            id: body.id,
            pos: body.pos,
            team: body.key.team,
            shape: body.shape,
            tags: body.key.tags,
        }
    }

    /// The unit of `row`.
    fn body((&id, &pos, &team, _, body, tags): TargetRow<'_>) -> LivingUnit {
        LivingUnit {
            id,
            pos,
            team,
            shape: Body::shape_of(body),
            tags: tags.map_or(TagSet::default(), |tags| tags.tags),
        }
    }
}
