use std::cmp::Ordering;

use bevy_ecs::query::{Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::Position;

use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::walls::Walls;
use crate::production::build_specs::{BuildSpec, PlacementCheck};
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::move_step::MoveStep;
use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::values::relation::Relation;
use crate::vision::seen_by::SeenBy;

/// What a placement tests a building's box against: the bounds, the walls, the static bodies,
/// and the living units, the walkers among them and what each team sees of them.
#[derive(SystemParam, Debug)]
pub(crate) struct Placement<'w, 's> {
    bounds: Res<'w, Bounds>,
    walls: Option<Res<'w, Walls>>,
    statics: Res<'w, BodyIndex>,
    relations: Res<'w, Relations>,
    metric: Res<'w, Metric>,
    units: Query<
        'w,
        's,
        (
            &'static Position,
            Option<&'static Body>,
            &'static Team,
            Option<&'static UnitTags>,
            Has<MoveStep>,
            Option<&'static SeenBy>,
        ),
        Without<Dead>,
    >,
}

impl Placement<'_, '_> {
    /// Whether `body`, the box of `spec`'s building, may stand at `at` for a builder of `team`:
    /// it lies within the bounds, and its inside shares no point with a wall of its layer, a
    /// static body of its layer, or the body on its layer of a walker that is not of a team
    /// friendly to `team` and that `team` sees, all of a match with no vision; and it meets its
    /// placement's rules. Touching is not overlap.
    pub(crate) fn passes(&self, team: Team, at: Position, body: Body, spec: BuildSpec<'_>) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            panic!("a building's body is a box");
        };
        let layer = body.layer();
        let walls = self.walls.as_deref();
        if !Walls::room_for(walls, *self.bounds, at, &boxed, layer) {
            return false;
        }
        let overlaps = |other: &IndexedBody| other.overlaps_box(at, &boxed);
        if self
            .statics
            .any_near(layer, at.get(), boxed.bound(), overlaps)
        {
            return false;
        }
        let blocked = self
            .units
            .iter()
            .any(|(&pos, other, &other_team, _, walks, seen)| {
                let Some(other) = other.filter(|other| walks && other.layer() == layer) else {
                    return false;
                };
                let inside = match other.shape() {
                    Shape::Circle(radius) => boxed.nearest(at, pos, radius) == Ordering::Less,
                    Shape::Box(other) => boxed.overlaps(at, &other, pos),
                };
                let friendly = self.relations.between(team, other_team) == Relation::Friendly;
                let sees = seen.is_none_or(|seen| seen.get().contains(team));
                inside && !friendly && sees
            });
        !blocked
            && spec
                .near
                .iter()
                .all(|&rule| self.meets(team, at, body, rule))
            && !spec
                .away
                .iter()
                .any(|&rule| self.meets(team, at, body, rule))
    }

    /// Whether a living unit that `rule`'s filter selects for a builder of `team` comes within
    /// its distance of `body` at `at`, by the reach rule.
    fn meets(&self, team: Team, at: Position, body: Body, rule: PlacementCheck) -> bool {
        self.units
            .iter()
            .any(|(&pos, other, &other_team, tags, ..)| {
                let relation = self.relations.between(team, other_team);
                let tags = tags.map(|tags| tags.tags).unwrap_or_default();
                rule.filter.selects(relation, tags)
                    && self.metric.reaches(
                        at,
                        body.shape(),
                        rule.distance,
                        pos,
                        Body::shape_of(other),
                    )
            })
    }
}
