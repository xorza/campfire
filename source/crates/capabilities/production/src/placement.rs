use std::ops::ControlFlow;

use bevy_ecs::query::{Allow, Has, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_geometry::Placed;
use campfire_math::{Num, Vec3};
use campfire_players::OnMatch;
use campfire_sim::{Position, Unpredicted};
use campfire_units::{Body, Dead, MapBook, MoveStep, UnitsNear, WallBook};
use campfire_values::{Relation, Relations, TagSet, Team, UnitTags};
use campfire_vision::{Fog, TeamSight, VisionBook};

use crate::build_book::{BuildSpec, PlacementCheck};

/// What a placement tests a building's body against: the bounds, the walls, whether the builder's
/// team sees its point, the static bodies, seen or not, and the living units near it that the
/// builder's team sees, by `S`, of a match with no vision every unit, the walkers among them,
/// each found by the unit grid. A client tests the units it holds and does not predict too, where
/// the server last had them.
#[derive(SystemParam, Debug)]
pub struct Placement<'w, 's, S: TeamSight> {
    walls: Option<Res<'w, WallBook>>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
    vision: Option<Res<'w, VisionBook>>,
    fog: Option<Res<'w, Fog>>,
    near: UnitsNear<'w>,
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
    /// Whether `body`, the box or the circle of `spec`'s building, may stand at `position` for a
    /// builder of `team`: it lies within the bounds, `team` saw its point in the last Vision stage,
    /// as Zero Hour refuses a build point under shroud (04-capabilities/production.md, A build
    /// point its team sees), and its inside shares no point with a wall of its layer, a static body
    /// of its layer, or the body on its layer of a walker that is not of a team friendly to `team`;
    /// and it meets its placement's rules. Each unit it tests is one `team` sees, as `SeenNear`
    /// finds them. Touching is not overlap (04-capabilities/production.md, A building may stand on
    /// a circle).
    pub fn passes(&self, team: Team, position: Vec3, body: Body, spec: BuildSpec<'_>) -> bool {
        let placed = Placed::new(position, body.shape());
        let bound = placed.shape.bound();
        let layer = body.layer();
        let walled = self
            .walls
            .as_deref()
            .is_some_and(|walls| walls.overlaps(placed, layer));
        if !self.map.bounds.holds(position, &placed.shape) || walled {
            return false;
        }
        let grid = self.vision.as_deref().and_then(VisionBook::grid);
        let unseen = |(fog, grid)| !S::sees_point(fog, grid, position, team);
        if self.fog.as_deref().zip(grid).is_some_and(unseen) {
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
        let seen = SeenNear::<S> { near, units, team };
        // A static body blocks seen or not; a walker only when the team sees it, and is no
        // friend's.
        let overlaps = |unit: &NearUnit<'_>, walks: bool| {
            unit.body
                .filter(|other| unit.walks == walks && other.layer() == layer)
                .is_some_and(|other| placed.overlaps(Placed::new(unit.position, other.shape())))
        };
        if seen.any_static(position, bound, |unit| overlaps(&unit, false)) {
            return false;
        }
        let blocked = seen.any(position, bound, |unit| {
            overlaps(&unit, true) && relations.between(team, unit.team) != Relation::Friendly
        });
        // A rule's unit is one its filter selects for a builder of `team`, of those `team` sees,
        // that comes within its distance of the body by the reach rule.
        let mut meets = |rule: &PlacementCheck| {
            let reach = bound.checked_add(rule.distance).unwrap_or(Num::MAX);
            seen.any(position, reach, |unit| {
                rule.filter
                    .selects(relations.between(team, unit.team), unit.tags)
                    && map.metric().reaches(
                        placed,
                        rule.distance,
                        Placed::new(unit.position, Body::shape_of(unit.body)),
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
    position: Vec3,
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
    near: &'p UnitsNear<'w>,
    units: &'p Query<'w, 's, PlacedUnit<S>, (Without<Dead>, Allow<Unpredicted>)>,
    team: Team,
}

/// Which of the units near a placement a search visits: those its builder's team sees, or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sighted {
    Seen,
    Any,
}

impl<S: TeamSight> SeenNear<'_, '_, '_, S> {
    /// Whether `found` holds of a living unit whose bound comes within `reach` of `position`, that
    /// the team sees, of a match with no vision every unit.
    fn any(&self, position: Vec3, reach: Num, found: impl FnMut(NearUnit<'_>) -> bool) -> bool {
        self.visit(position, reach, Sighted::Seen, found)
    }

    /// Whether `found` holds of a living unit whose bound comes within `reach` of `position`,
    /// whether the team sees it or not.
    fn any_static(
        &self,
        position: Vec3,
        reach: Num,
        found: impl FnMut(NearUnit<'_>) -> bool,
    ) -> bool {
        self.visit(position, reach, Sighted::Any, found)
    }

    /// Whether `found` holds of a living unit whose bound comes within `reach` of `position`, of
    /// those `sighted` names.
    fn visit(
        &self,
        position: Vec3,
        reach: Num,
        sighted: Sighted,
        mut found: impl FnMut(NearUnit<'_>) -> bool,
    ) -> bool {
        let (units, team) = (self.units, self.team);
        let mut any = false;
        self.near.current().visit_near(position, reach, |indexed| {
            let Ok((at, body, &of_team, tags, walks, seen)) = units.get(indexed.key) else {
                return ControlFlow::Continue(());
            };
            if sighted == Sighted::Any || S::sees(seen, team).unwrap_or(true) {
                any = found(NearUnit {
                    position: at.get(),
                    body,
                    team: of_team,
                    tags: tags.map_or_default(|tags| tags.tags),
                    walks,
                });
            }
            if any {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        any
    }
}
