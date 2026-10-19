use std::fmt;

use bevy::camera::Camera;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::system::{Query, Res, Single, SystemParam};
use bevy::math::primitives::InfinitePlane3d;
use bevy::math::{Ray3d, Rect, Vec2, Vec3};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::transform::components::{GlobalTransform, Transform};
use bevy::ui::Node;
use bevy::window::{PrimaryWindow, Window};
use campfire_common::Seat;
use campfire_sim::{StableId, Unpredicted};
use campfire_units::{Dead, Owner};
use campfire_values::Team;

use crate::own_player::OwnPlayer;
use crate::view::client_data::ClientData;
use crate::view::drawing::Drawing;
use crate::view::footing;
use crate::view::look::Shape;

/// What the cursor points at, as the player sees the match: the ground point under it, and the
/// living unit whose drawing the cursor's ray meets first, as Zero Hour picks a drawable by the
/// ray through the cursor.
#[derive(SystemParam)]
pub(crate) struct Pointer<'w, 's> {
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    own: OwnPlayer<'w, 's>,
    units: LivingUnits<'w, 's>,
    drawings: Query<'w, 's, (&'static Transform, &'static Shape)>,
    data: Res<'w, ClientData>,
    hovers: Res<'w, HoverMap>,
    nodes: Query<'w, 's, (), With<Node>>,
}

/// The living units the client holds: each one's stable id, its team, the player who controls
/// it, and its drawing.
type LivingUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Team,
        Option<&'static Owner>,
        &'static Drawing,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// A unit the player may point at: its stable id, its team, and the player who controls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pointed {
    pub(crate) id: StableId,
    pub(crate) team: Team,
    pub(crate) owner: Option<Seat>,
}

impl Pointer<'_, '_> {
    /// Whether the cursor is over a node of the UI, the shop panel or the inventory bar, which
    /// takes its clicks from the match.
    pub(crate) fn is_over_ui(&self) -> bool {
        let hits = self.hovers.get(&PointerId::Mouse);
        hits.is_some_and(|hits| hits.keys().any(|&hit| self.nodes.contains(hit)))
    }

    /// The cursor's point of the window, in logical pixels from its top left; none when the
    /// cursor is off the window.
    pub(crate) fn cursor(&self) -> Option<Vec2> {
        self.window.cursor_position()
    }

    /// The point of the ground under the cursor, when the cursor is over the window and the ground:
    /// the drawn ground of a map with heights, else the plane at height 0.
    pub(crate) fn ground(&self) -> Option<Vec3> {
        let ray = self.ray()?;
        match self.data.heights.as_ref() {
            Some(heights) => heights.hit(ray.origin, *ray.direction),
            None => ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)),
        }
    }

    /// The living unit under the cursor, of those not in `except`, a list in stable id order.
    pub(crate) fn unit_under(&self, except: &[StableId]) -> Option<Pointed> {
        debug_assert!(
            except.is_sorted(),
            "a binary search needs the list in order"
        );
        self.nearest(|unit| except.binary_search(&unit.id).is_err())
    }

    /// The living unit of the player's own under the cursor.
    pub(crate) fn own_unit_under(&self) -> Option<StableId> {
        let seat = self.own.seat();
        let unit = self.nearest(|unit| unit.owned_by(seat))?;
        Some(unit.id)
    }

    /// The ray from the camera through the cursor, when the cursor is over the window.
    fn ray(&self) -> Option<Ray3d> {
        let (camera, place) = *self.camera;
        camera.viewport_to_world(place, self.cursor()?).ok()
    }

    /// Adds to `out` each living unit of the player's own whose ground point shows within `rect`
    /// of the window, in logical pixels, edges included.
    pub(crate) fn own_units_within(&self, rect: Rect, out: &mut Vec<StableId>) {
        let (camera, place) = *self.camera;
        let shown = self
            .drawn()
            .map(|(unit, at, _)| (unit, camera.world_to_viewport(place, at).ok()));
        out.extend(shown_within(rect, self.own.seat(), shown));
    }

    /// Of the living units, the one under the cursor that `keep` keeps, as `nearest_hit` picks.
    fn nearest(&self, keep: impl Fn(Pointed) -> bool) -> Option<Pointed> {
        let ray = self.ray()?;
        let drawn = self.drawn().filter(|&(unit, _, _)| keep(unit));
        nearest_hit(ray.origin, *ray.direction, drawn)
    }

    /// Each living unit with a drawing: where the drawing stands, and its shape.
    fn drawn(&self) -> impl Iterator<Item = (Pointed, Vec3, Shape)> {
        self.units.iter().filter_map(|(&id, &team, owner, drawn)| {
            let (transform, shape) = self.drawings.get(drawn.root()).ok()?;
            let unit = Pointed {
                id,
                team,
                owner: owner.map(|owner| owner.seat()),
            };
            Some((unit, transform.translation, *shape))
        })
    }
}

impl Pointed {
    /// Whether the player of `seat` controls it; a client of no slot, a spectator's, controls no
    /// unit.
    fn owned_by(self, seat: Option<Seat>) -> bool {
        seat.is_some() && self.owner == seat
    }
}

/// Of `units`, each with the point of the window its ground point shows at, if it shows, the
/// stable ids of those the player of `seat` controls whose point lies within `rect`, edges
/// included.
fn shown_within(
    rect: Rect,
    seat: Option<Seat>,
    units: impl Iterator<Item = (Pointed, Option<Vec2>)>,
) -> impl Iterator<Item = StableId> {
    units
        .filter(move |&(unit, shown)| {
            unit.owned_by(seat) && shown.is_some_and(|shown| rect.contains(shown))
        })
        .map(|(unit, _)| unit.id)
}

/// Of the drawings, each a unit's base point and its shape, the unit whose drawing the ray from
/// `origin` along `direction` meets first; the lower stable id on a tie.
fn nearest_hit(
    origin: Vec3,
    direction: Vec3,
    drawings: impl Iterator<Item = (Pointed, Vec3, Shape)>,
) -> Option<Pointed> {
    drawings
        .filter_map(|(unit, base, shape)| Some((unit, entry(origin, direction, base, shape)?)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)))
        .map(|(unit, _)| unit)
}

/// How far along the ray from `origin` along `direction` it first meets the drawing that stands
/// at `base` with `shape`: its footing raised `shape.height`, a circle's as a cylinder, a box's as
/// a box; none when it misses, or meets it only behind the origin.
fn entry(origin: Vec3, direction: Vec3, base: Vec3, shape: Shape) -> Option<f32> {
    let from = Vec2::new(origin.x - base.x, origin.z - base.z);
    let way = Vec2::new(direction.x, direction.z);
    let flat = shape.footing.crossed(from, way)?;
    let half = shape.height / 2.0;
    let high = footing::span(origin.y - base.y - half, direction.y, half)?;
    let near = flat.start.max(high.start).max(0.0);
    let far = flat.end.min(high.end);
    (near <= far).then_some(near)
}

/// A system param holds borrows of the world, whose queries print nothing of use.
impl fmt::Debug for Pointer<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pointer")
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use campfire_sim::IdAllocator;

    use super::*;
    use crate::view::footing::Footing;

    #[test]
    fn the_unit_whose_drawing_the_ray_meets_first_is_the_one_pointed_at() {
        let mut ids = IdAllocator::default();
        let mut unit = || Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner: None,
        };
        let circle = |radius, height| Shape {
            radius,
            height,
            footing: Footing::Circle(radius),
        };
        // An avatar of 0.5 m and 2 m tall at the origin, a creep of 0.5 m 3 m behind it, and a
        // unit of a higher id drawn where the avatar stands.
        let [avatar, creep, twin] = [unit(), unit(), unit()];
        let drawings = [
            (avatar, Vec3::ZERO, circle(0.5, 2.0)),
            (creep, Vec3::new(0.0, 0.0, -3.0), circle(0.5, 2.0)),
            (twin, Vec3::ZERO, circle(0.5, 2.0)),
        ];
        let pick = |origin: Vec3, direction: Vec3, drawings: &[(Pointed, Vec3, Shape)]| {
            nearest_hit(origin, direction, drawings.iter().copied())
        };
        // A ray at 1 m along -z from z = 10 meets the avatar's side at z = 0.5, 9.5 along, and
        // the creep's at 12.5: the avatar, the lower id of the two drawn there.
        let level = (Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z);
        assert_eq!(
            entry(level.0, level.1, Vec3::ZERO, circle(0.5, 2.0)),
            Some(9.5)
        );
        assert_eq!(pick(level.0, level.1, &drawings), Some(avatar));
        // At 3 m, over their tops, it meets none; half a meter aside, 0.75 off the axis, none.
        assert_eq!(
            pick(Vec3::new(0.0, 3.0, 10.0), Vec3::NEG_Z, &drawings),
            None
        );
        assert_eq!(
            pick(Vec3::new(0.75, 1.0, 10.0), Vec3::NEG_Z, &drawings),
            None
        );
        // Straight down onto the creep: it meets the creep's top at 2 m, 8 along from 10 m up.
        let down = (Vec3::new(0.0, 10.0, -3.0), Vec3::NEG_Y);
        assert_eq!(pick(down.0, down.1, &drawings), Some(creep));
        assert_eq!(
            entry(down.0, down.1, Vec3::new(0.0, 0.0, -3.0), circle(0.5, 2.0)),
            Some(8.0)
        );
        // A ray that starts inside a drawing meets it where it starts.
        assert_eq!(
            entry(
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::NEG_Z,
                Vec3::ZERO,
                circle(0.5, 2.0)
            ),
            Some(0.0)
        );

        // A building of half sides 2 m and 10 m tall: a level ray at 5 m from z = 20 meets its
        // wall at z = 2, 18 along, where its footing lies under no ground point the ray crosses,
        // and the creep behind it is hidden.
        let building = unit();
        let tall = Shape {
            radius: 2.0,
            height: 10.0,
            footing: Footing::Box {
                half: [2.0, 2.0],
                yaw: 0.0,
            },
        };
        let behind = (creep, Vec3::new(0.0, 0.0, -5.0), circle(0.5, 2.0));
        let city = [(building, Vec3::ZERO, tall), behind];
        let wall = (Vec3::new(0.0, 5.0, 20.0), Vec3::NEG_Z);
        assert_eq!(entry(wall.0, wall.1, Vec3::ZERO, tall), Some(18.0));
        assert_eq!(pick(wall.0, wall.1, &city), Some(building));
        assert_eq!(pick(Vec3::new(0.0, 11.0, 20.0), Vec3::NEG_Z, &city), None);
        // Turned a quarter, a box of half sides 2 and 0.5 reaches 2 along z and 0.5 along x: a
        // ray along x at z = 1.9 meets it, one at z = 2.1 passes by.
        let turned = Shape {
            radius: 2.0,
            height: 1.0,
            footing: Footing::Box {
                half: [2.0, 0.5],
                yaw: FRAC_PI_2,
            },
        };
        let along = |z| entry(Vec3::new(-10.0, 0.5, z), Vec3::X, Vec3::ZERO, turned);
        assert!(along(1.9).is_some());
        assert_eq!(along(2.1), None);
    }

    #[test]
    fn a_drag_selects_the_players_own_units_shown_within_its_rectangle() {
        let mut ids = IdAllocator::default();
        let (own, other) = (Seat::new(0), Seat::new(1));
        let mut unit = |owner: Option<Seat>| Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner,
        };
        // A rectangle from (10, 20) to (110, 70), dragged from its bottom right corner.
        let rect = Rect::from_corners(Vec2::new(110.0, 70.0), Vec2::new(10.0, 20.0));
        let [inside, edge, outside, behind, others, unowned] = [
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(own)),
            unit(Some(other)),
            unit(None),
        ];
        let units = [
            // Inside; on the left edge; 0.5 px past the bottom edge; behind the camera, shown
            // nowhere.
            (inside, Some(Vec2::new(60.0, 45.0))),
            (edge, Some(Vec2::new(10.0, 50.0))),
            (outside, Some(Vec2::new(60.0, 70.5))),
            (behind, None),
            // Inside, another player's, and no player's.
            (others, Some(Vec2::new(60.0, 45.0))),
            (unowned, Some(Vec2::new(60.0, 45.0))),
        ];
        let within = |slot| shown_within(rect, slot, units.into_iter()).collect::<Vec<_>>();
        assert_eq!(within(Some(own)), [inside.id, edge.id]);
        assert_eq!(within(Some(other)), [others.id]);
        // A spectator, of no slot, controls none, not even a unit no player controls.
        assert_eq!(within(None), []);
    }
}
