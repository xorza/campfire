use std::cmp::Ordering;

use bevy_ecs::query::{Allow, Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_geometry::{Placed, Shape};
use campfire_math::{Num, Vec3};
use campfire_navigation::{BodyIndex, IndexedBody, NavigationBook};
use campfire_players::OnMatch;
use campfire_sim::{Position, Unpredicted};
use campfire_units::{Body, Dead, MapBook, MoveStep, UnitsNear};
use campfire_values::{Relation, Relations, TagSet, Team, UnitTags};
use campfire_vision::TeamSight;

use crate::build_book::{BuildSpec, PlacementCheck};

/// What a placement tests a building's box against: the bounds, the walls, the static bodies,
/// and the living units near it that the builder's team sees, by `S`, of a match with no vision
/// every unit, the walkers among them, found by the unit grid. A client tests the units it holds
/// and does not predict too, where the server last had them.
#[derive(SystemParam, Debug)]
pub struct Placement<'w, 's, S: TeamSight> {
    navigation: Option<Res<'w, NavigationBook>>,
    statics: Res<'w, BodyIndex>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
    near: UnitsNear<'w, 's>,
    units: Query<'w, 's, PlacedUnit<S>, (Without<Dead>, Allow<Unpredicted>)>,
}

/// What a placement reads of a living unit near it: where it stands, its body, its team and
/// tags, whether it walks, and what `S` reads to tell the teams that see it.
type PlacedUnit<S> = (
    &'static Position,
    Option<&'static Body>,
    &'static Team,
    Option<&'static UnitTags>,
    Has<MoveStep>,
    <S as TeamSight>::Seen,
);

impl<S: TeamSight> Placement<'_, '_, S> {
    /// Whether `body`, the box of `spec`'s building, may stand at `pos` for a builder of `team`:
    /// it lies within the bounds, and its inside shares no point with a wall of its layer, a
    /// static body of its layer, or the body on its layer of a walker that is not of a team
    /// friendly to `team`; and it meets its placement's rules. Each unit it tests is one `team`
    /// sees, as `SeenNear` finds them. Touching is not overlap.
    pub fn passes(&mut self, team: Team, pos: Vec3, body: Body, spec: BuildSpec<'_>) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            panic!("a building's body is a box");
        };
        let layer = body.layer();
        let walled = self
            .navigation
            .as_deref()
            .is_some_and(|book| book.walls.overlaps_box(pos, &boxed, layer));
        if !self.map.bounds.holds_box(pos, &boxed) || walled {
            return false;
        }
        let overlaps = |other: &IndexedBody| other.overlaps_box(pos, &boxed);
        if self.statics.any_near(layer, pos, boxed.bound(), overlaps) {
            return false;
        }
        let Self {
            relations,
            map,
            near,
            units,
            ..
        } = self;
        let relations = relations.get();
        let mut seen = SeenNear::<S> { near, units, team };
        let blocked = seen.any(pos, boxed.bound(), |unit| {
            let Some(other) = unit
                .body
                .filter(|other| unit.walks && other.layer() == layer)
            else {
                return false;
            };
            let inside = match other.shape() {
                Shape::Circle(radius) => {
                    boxed.nearest(pos, unit.pos, radius.get()) == Ordering::Less
                }
                Shape::Box(other) => boxed.overlaps(pos, &other, unit.pos),
            };
            inside && relations.between(team, unit.team) != Relation::Friendly
        });
        // A rule's unit is one its filter selects for a builder of `team`, of those `team` sees,
        // that comes within its distance of the box by the reach rule.
        let mut meets = |rule: &PlacementCheck| {
            let reach = boxed.bound().checked_add(rule.distance).unwrap_or(Num::MAX);
            seen.any(pos, reach, |unit| {
                rule.filter
                    .selects(relations.between(team, unit.team), unit.tags)
                    && map.metric.reaches(
                        Placed::new(pos, body.shape()),
                        rule.distance,
                        Placed::new(unit.pos, Body::shape_of(unit.body)),
                    )
            })
        };
        !blocked && spec.near.iter().all(&mut meets) && !spec.away.iter().any(meets)
    }
}

/// A living unit near a placement, as its tests read it: where it stands, its body, its team and
/// tags, and whether it walks.
#[derive(Debug, Clone, Copy)]
struct NearUnit<'a> {
    pos: Vec3,
    body: Option<&'a Body>,
    team: Team,
    tags: TagSet,
    walks: bool,
}

/// The search of the living units near a placement that its builder's team sees, by the unit
/// grid: the one search every test of a placement reads, so no test tells the builder of a unit
/// its team does not see, and none visits a unit far off.
#[derive(Debug)]
struct SeenNear<'p, 'w, 's, S: TeamSight> {
    near: &'p mut UnitsNear<'w, 's>,
    units: &'p Query<'w, 's, PlacedUnit<S>, (Without<Dead>, Allow<Unpredicted>)>,
    team: Team,
}

impl<S: TeamSight> SeenNear<'_, '_, '_, S> {
    /// Whether `found` holds of a living unit whose bound comes within `reach` of `pos`, that
    /// the team sees, of a match with no vision every unit.
    fn any(&mut self, pos: Vec3, reach: Num, mut found: impl FnMut(NearUnit<'_>) -> bool) -> bool {
        let (units, team) = (self.units, self.team);
        let mut any = false;
        self.near.current().visit_near(pos, reach, |indexed| {
            if any {
                return;
            }
            let Ok((at, body, &of_team, tags, walks, seen)) = units.get(indexed.key) else {
                return;
            };
            if S::sees(seen, team).unwrap_or(true) {
                any = found(NearUnit {
                    pos: at.get(),
                    body,
                    team: of_team,
                    tags: tags.map_or_default(|tags| tags.tags),
                    walks,
                });
            }
        });
        any
    }
}
