use std::cmp::Ordering;

use bevy_ecs::query::{Allow, Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_core::{Body, Dead, MapBook, MoveStep, OnMatch, Relations, Team, UnitTags};
use campfire_geometry::Shape;
use campfire_navigation::{BodyIndex, IndexedBody, NavigationBook, Walls};
use campfire_sim::{Position, Unpredicted};
use campfire_values::Relation;
use campfire_vision::TeamSight;

use crate::build_book::{BuildSpec, PlacementCheck};

/// What a placement tests a building's box against: the bounds, the walls, the static bodies,
/// and the living units, the walkers among them and what each team sees of them, by `S`. A
/// client tests the units it holds and does not predict too, where the server last had them.
#[derive(SystemParam, Debug)]
pub(crate) struct Placement<'w, 's, S: TeamSight> {
    navigation: Option<Res<'w, NavigationBook>>,
    statics: Res<'w, BodyIndex>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
    units: Query<
        'w,
        's,
        (
            &'static Position,
            Option<&'static Body>,
            &'static Team,
            Option<&'static UnitTags>,
            Has<MoveStep>,
            <S as TeamSight>::Seen,
        ),
        (Without<Dead>, Allow<Unpredicted>),
    >,
}

impl<S: TeamSight> Placement<'_, '_, S> {
    /// Whether `body`, the box of `spec`'s building, may stand at `pos` for a builder of `team`:
    /// it lies within the bounds, and its inside shares no point with a wall of its layer, a
    /// static body of its layer, or the body on its layer of a walker that is not of a team
    /// friendly to `team` and that `team` sees, all of a match with no vision; and it meets its
    /// placement's rules. Touching is not overlap.
    pub(crate) fn passes(
        &self,
        team: Team,
        pos: Position,
        body: Body,
        spec: BuildSpec<'_>,
    ) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            panic!("a building's body is a box");
        };
        let layer = body.layer();
        let walls = self.navigation.as_deref().map(|book| &book.walls);
        if !Walls::room_for(walls, self.map.bounds, pos, &boxed, layer) {
            return false;
        }
        let overlaps = |other: &IndexedBody| other.overlaps_box(pos, &boxed);
        if self
            .statics
            .any_near(layer, pos.get(), boxed.bound(), overlaps)
        {
            return false;
        }
        let blocked = self
            .units
            .iter()
            .any(|(&other_pos, other, &other_team, _, walks, seen)| {
                let Some(other) = other.filter(|other| walks && other.layer() == layer) else {
                    return false;
                };
                let inside = match other.shape() {
                    Shape::Circle(radius) => {
                        boxed.nearest(pos, other_pos, radius) == Ordering::Less
                    }
                    Shape::Box(other) => boxed.overlaps(pos, &other, other_pos),
                };
                let friendly = self.relations.get().between(team, other_team) == Relation::Friendly;
                let sees = S::sees(seen, team).unwrap_or(true);
                inside && !friendly && sees
            });
        !blocked
            && spec
                .near
                .iter()
                .all(|&rule| self.meets(team, pos, body, rule))
            && !spec
                .away
                .iter()
                .any(|&rule| self.meets(team, pos, body, rule))
    }

    /// Whether a living unit that `rule`'s filter selects for a builder of `team` comes within
    /// its distance of `body` at `pos`, by the reach rule.
    fn meets(&self, team: Team, pos: Position, body: Body, rule: PlacementCheck) -> bool {
        self.units
            .iter()
            .any(|(&other_pos, other, &other_team, tags, ..)| {
                let relation = self.relations.get().between(team, other_team);
                let tags = tags.map(|tags| tags.tags).unwrap_or_default();
                rule.filter.selects(relation, tags)
                    && self.map.metric.reaches(
                        pos,
                        body.shape(),
                        rule.distance,
                        other_pos,
                        Body::shape_of(other),
                    )
            })
    }
}
