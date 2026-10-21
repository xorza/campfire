use std::cmp::Ordering;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_geometry::{Metric, Placed, Volume};
use campfire_math::{Num, Vec3};
use campfire_players::OnMatch;
use campfire_script::LivingUnit;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Body, Dead, Life, MapBook};
use campfire_values::{Block, Distance, Relation, Relations, TagSet, Team, UnitTags};

use crate::action_range::ActionRange;
use crate::range_fit::RangeFit;
use crate::rank_values::RankValues;

/// The units an attack may target: units with a life that live by `stats`' living test, whose
/// tags let them be targets, those a client holds and does not predict among them, where the
/// server last had them. Every capability that chooses or checks a target goes through it, so all
/// agree on what a valid target is.
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
        self.map.metric()
    }

    /// How `of` regards `other`.
    pub fn relation(&self, of: Team, other: Team) -> Relation {
        self.relations.get().between(of, other)
    }

    /// Whether `range` from the body `from` reaches `target`: within range in the map's metric,
    /// exactly, from the edge of the one body to the edge of the other.
    pub fn reaches(&self, from: Placed, range: Num, target: &LivingUnit) -> bool {
        let target = Placed::new(target.position, target.shape);
        self.map.metric().reaches(from, range, target)
    }

    /// How the body `to` lies from the body `from` against `range` and `min_range`, exactly, from
    /// edge to edge: beyond a range of meters, nearer than the minimum, or within its reach.
    pub fn reach(
        &self,
        from: Placed,
        range: ActionRange,
        min_range: Option<Distance>,
        to: Placed,
    ) -> RangeFit {
        let metric = self.map.metric();
        if let ActionRange::Meters(range) = range
            && metric.order(from, range.get(), to) == Ordering::Greater
        {
            return RangeFit::Far;
        }
        if min_range.is_some_and(|min| metric.order(from, min.get(), to) == Ordering::Less) {
            return RangeFit::Near;
        }
        RangeFit::Within
    }

    /// Where the body `from` walks to bring the body `to`, which lies `fit` from it, within the
    /// reach of `values`: to `to`'s place beyond its range, and straight away from it to halfway
    /// between its minimum and its range nearer than its minimum; none within reach.
    pub fn walk_to(
        &self,
        from: Placed,
        to: Placed,
        values: &RankValues,
        fit: RangeFit,
    ) -> Option<Vec3> {
        match fit {
            RangeFit::Within => None,
            RangeFit::Far => Some(to.position),
            RangeFit::Near => values.midway().map(|gap| self.away(from, to, gap)),
        }
    }

    /// The point the body `from` walks to straight away from the body `to`, so that their bounds
    /// lie `gap` apart, as Zero Hour's `computeApproachTarget` steps a unit back from a victim
    /// within its weapon's minimum range; along the first axis from bodies at one place.
    fn away(&self, from: Placed, to: Placed, gap: Num) -> Vec3 {
        let metric = self.map.metric();
        let way = metric
            .direction(metric.offset(to.position, from.position))
            .unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO));
        to.position + way * (gap + from.shape.bound() + to.shape.bound())
    }

    /// Whether `range` from the body `from` reaches the point `position`, from the edge of the
    /// body, exactly.
    pub(crate) fn reaches_point(&self, from: Placed, range: Num, position: Vec3) -> bool {
        self.map
            .metric()
            .reaches(from, range, Placed::point(position))
    }

    /// The point `step` from `from` toward `to` in the map's metric, or `to` when it is nearer.
    pub fn toward(&self, from: Vec3, to: Vec3, step: Num) -> Vec3 {
        self.map.metric().step_toward(from, to, step)
    }

    /// `target`, when it is a living unit that may be a target.
    pub fn living(&self, target: StableId) -> Option<LivingUnit> {
        self.target_at(self.index.get(target)?)
    }

    /// Whether a unit, `dead` or not, with `life` and `tags`, may be a target: it has a life, it
    /// lives by `stats`' living test, so a life at 0 that has not died yet is none, and its tags
    /// let it be one. This is the one rule of a target, which the script view reads too.
    pub fn targetable(dead: bool, life: Option<&Life>, tags: Option<&UnitTags>) -> bool {
        life.is_some()
            && campfire_units::lives(dead, life)
            && !tags.is_some_and(|tags| tags.properties.blocks(Block::Target))
    }

    /// Whether `target` lives by `stats`' living test, whatever its tags let it be.
    pub fn lives(&self, target: StableId) -> bool {
        self.index
            .get(target)
            .and_then(|entity| self.living_at(entity))
            .is_some()
    }

    /// The unit of `entity`, when a living unit with the life pool, whose tags may block it as a
    /// target: one an area reaches.
    pub fn living_at(&self, entity: Entity) -> Option<LivingUnit> {
        let row = self.units.get(entity).ok()?;
        let (_, _, _, life, _, _) = row;
        campfire_units::lives(false, Some(life)).then(|| Targets::body(row))
    }

    /// The unit of `entity`, when a living unit that may be a target.
    pub fn target_at(&self, entity: Entity) -> Option<LivingUnit> {
        let row = self.units.get(entity).ok()?;
        let (_, _, _, life, _, tags) = row;
        // The query holds no unit that died.
        Targets::targetable(false, Some(life), tags).then(|| Targets::body(row))
    }

    /// The volume of `target`, when it is a living unit that may be a target, as `Body::volume`
    /// gives it.
    pub fn volume_of(&self, target: StableId) -> Option<Volume> {
        let entity = self.index.get(target)?;
        self.target_at(entity)?;
        self.volume_at(entity)
    }

    /// The volume of the unit of `entity`, of any unit it holds, whatever its life: what a caller
    /// found by `target_at` or `living_at`.
    pub fn volume_at(&self, entity: Entity) -> Option<Volume> {
        let (_, position, _, _, body, _) = self.units.get(entity).ok()?;
        Some(Body::volume(body, position.get(), &self.map))
    }

    /// The map the units stand on.
    pub fn map(&self) -> &MapBook {
        &self.map
    }

    /// The unit of `row`.
    fn body((&id, &position, &team, _, body, tags): TargetRow<'_>) -> LivingUnit {
        LivingUnit {
            id,
            position: position.get(),
            team,
            shape: Body::shape_of(body),
            tags: tags.map_or_else(TagSet::default, |tags| tags.tags),
        }
    }
}
