use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_geometry::{Metric, Shape};
use campfire_math::Num;
use campfire_players::OnMatch;
use campfire_script::LivingUnit;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_stats::Life;
use campfire_units::{Body, Dead, MapBook};
use campfire_values::{Block, Relation, Relations, TagSet, Team, UnitTags};

/// The units an attack may target: living units with a life whose tags let them be
/// targets, those a client holds and does not predict among them, where the server last had
/// them. Every capability that chooses or checks a target goes through it, so all agree on what a
/// valid target is.
#[derive(SystemParam, Debug)]
pub struct Targets<'w, 's> {
    index: Res<'w, EntityIndex>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
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
    pub fn enemy(&self, team: Team, target: StableId) -> Option<LivingUnit> {
        self.living(target)
            .filter(|unit| self.relation(team, unit.team).may_attack())
    }

    pub fn metric(&self) -> Metric {
        self.map.metric
    }

    /// How `of` regards `other`.
    pub fn relation(&self, of: Team, other: Team) -> Relation {
        self.relations.get().between(of, other)
    }

    /// Whether `range` from a unit at `from` of body `shape` reaches `target`: within range in
    /// the map's metric, exactly, from the edge of the one body to the edge of the other.
    pub fn reaches(&self, from: Position, shape: Shape, range: Num, target: &LivingUnit) -> bool {
        self.map
            .metric
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
        self.map
            .metric
            .reaches(from, shape, range, pos, Shape::POINT)
    }

    /// The point `step` from `from` toward `to` in the map's metric, or `to` when it is nearer.
    pub fn toward(&self, from: Position, to: Position, step: Num) -> Position {
        self.map.metric.step_toward(from, to, step)
    }

    /// `target`, when it is a living unit that may be a target.
    pub fn living(&self, target: StableId) -> Option<LivingUnit> {
        self.target_at(self.index.get(target)?)
    }

    /// Whether a unit that has not died, with `life` and `tags`, may be a target: it has a life,
    /// and its tags let it be one. This is the one rule of a target, which the script view reads
    /// too.
    pub fn targetable(life: Option<&Life>, tags: Option<&UnitTags>) -> bool {
        life.is_some() && !UnitTags::properties_of(tags).blocks(Block::Target)
    }

    /// The unit of `entity`, when a living unit with the life pool, whose tags may block it as a
    /// target: one an area reaches.
    pub fn living_at(&self, entity: Entity) -> Option<LivingUnit> {
        self.units.get(entity).ok().map(Targets::body)
    }

    /// The unit of `entity`, when a living unit that may be a target.
    pub fn target_at(&self, entity: Entity) -> Option<LivingUnit> {
        let row = self.units.get(entity).ok()?;
        let (_, _, _, life, _, tags) = row;
        Targets::targetable(Some(life), tags).then(|| Targets::body(row))
    }

    /// The unit of `row`.
    fn body((&id, &pos, &team, _, body, tags): TargetRow<'_>) -> LivingUnit {
        LivingUnit {
            id,
            pos,
            team,
            shape: Body::shape_of(body),
            tags: tags.map_or_else(TagSet::default, |tags| tags.tags),
        }
    }
}
