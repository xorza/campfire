use std::fmt;

use bevy::camera::Camera;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::system::{Query, Res, Single, SystemParam};
use bevy::math::primitives::InfinitePlane3d;
use bevy::math::{Rect, Vec2, Vec3};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::transform::components::{GlobalTransform, Transform};
use bevy::ui::Node;
use bevy::window::{PrimaryWindow, Window};
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, Unpredicted};
use campfire_units::{Dead, Owner};
use campfire_values::Team;

use crate::own_player::OwnPlayer;
use crate::view::client_data::ClientData;
use crate::view::drawing::Drawing;
use crate::view::footing::Footing;
use crate::view::look::Shape;

/// What the cursor points at, as the player sees the match: the ground point under it, and the
/// living unit drawn there.
#[derive(SystemParam)]
pub(crate) struct Pointer<'w, 's> {
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    own: OwnPlayer<'w, 's>,
    units: LivingUnits<'w, 's>,
    drawings: Query<'w, 's, (&'static Transform, &'static Shape)>,
    data: Res<'w, ClientData>,
    hovers: Option<Res<'w, HoverMap>>,
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
    pub(crate) owner: Option<PlayerSlot>,
}

impl Pointer<'_, '_> {
    /// Whether the cursor is over a node of the UI, the shop panel or the inventory bar, which
    /// takes its clicks from the match.
    pub(crate) fn over_ui(&self) -> bool {
        let hits = self
            .hovers
            .as_deref()
            .and_then(|hovers| hovers.get(&PointerId::Mouse));
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
        let (camera, place) = *self.camera;
        let ray = camera.viewport_to_world(place, self.cursor()?).ok()?;
        match self.data.heights.as_ref() {
            Some(heights) => heights.hit(ray.origin, *ray.direction),
            None => ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)),
        }
    }

    /// The living unit drawn over `point` of the ground, of those not in `except`, a list in
    /// stable id order.
    pub(crate) fn unit_at(&self, point: Vec3, except: &[StableId]) -> Option<Pointed> {
        debug_assert!(
            except.is_sorted(),
            "a binary search needs the list in order"
        );
        self.nearest(point, |unit| except.binary_search(&unit.id).is_err())
    }

    /// The living unit of the player's own drawn over `point` of the ground.
    pub(crate) fn own_unit_at(&self, point: Vec3) -> Option<StableId> {
        let slot = self.own.slot();
        let unit = self.nearest(point, |unit| unit.owned_by(slot))?;
        Some(unit.id)
    }

    /// Adds to `out` each living unit of the player's own whose ground point shows within `rect`
    /// of the window, in logical pixels, edges included.
    pub(crate) fn own_units_within(&self, rect: Rect, out: &mut Vec<StableId>) {
        let (camera, place) = *self.camera;
        let shown = self
            .drawn()
            .map(|(unit, at, _)| (unit, camera.world_to_viewport(place, at).ok()));
        out.extend(shown_within(rect, self.own.slot(), shown));
    }

    /// Of the living units, the one drawn over `point` that `keep` keeps, as `nearest_over` picks.
    fn nearest(&self, point: Vec3, keep: impl Fn(Pointed) -> bool) -> Option<Pointed> {
        let drawn = self
            .drawn()
            .filter(|&(unit, _, _)| keep(unit))
            .map(|(unit, at, footing)| (unit, Vec2::new(at.x, at.z), footing));
        nearest_over(Vec2::new(point.x, point.z), drawn)
    }

    /// Each living unit with a drawing: where the drawing stands, and what it covers of the
    /// ground.
    fn drawn(&self) -> impl Iterator<Item = (Pointed, Vec3, Footing)> {
        self.units.iter().filter_map(|(&id, &team, owner, drawn)| {
            let (transform, shape) = self.drawings.get(drawn.root()).ok()?;
            let unit = Pointed {
                id,
                team,
                owner: owner.map(|owner| owner.slot()),
            };
            Some((unit, transform.translation, shape.footing))
        })
    }
}

impl Pointed {
    /// Whether the player of `slot` controls it; a client of no slot, a spectator's, controls no
    /// unit.
    fn owned_by(self, slot: Option<PlayerSlot>) -> bool {
        slot.is_some() && self.owner == slot
    }
}

/// Of `units`, each with the point of the window its ground point shows at, if it shows, the
/// stable ids of those the player of `slot` controls whose point lies within `rect`, edges
/// included.
fn shown_within(
    rect: Rect,
    slot: Option<PlayerSlot>,
    units: impl Iterator<Item = (Pointed, Option<Vec2>)>,
) -> impl Iterator<Item = StableId> {
    units
        .filter(move |&(unit, shown)| {
            unit.owned_by(slot) && shown.is_some_and(|shown| rect.contains(shown))
        })
        .map(|(unit, _)| unit.id)
}

/// Of the drawings on the ground, each a unit's center and what it covers, the unit whose drawing
/// covers `point` and whose center is nearest it; the lower stable id on a tie.
fn nearest_over(
    point: Vec2,
    drawings: impl Iterator<Item = (Pointed, Vec2, Footing)>,
) -> Option<Pointed> {
    drawings
        .filter(|&(_, center, footing)| footing.covers(point - center))
        .map(|(unit, center, _)| (unit, point.distance_squared(center)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)))
        .map(|(unit, _)| unit)
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

    #[test]
    fn the_nearest_unit_whose_drawing_holds_the_point_is_the_one_pointed_at() {
        let mut ids = IdAllocator::default();
        let mut unit = || Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner: None,
        };
        // An avatar of 0.5 m at the origin, a unit of 1 m at x = 1, a creep of 0.35 m at x = 0.25,
        // an avatar at x = 4, and a unit of a higher id drawn where the first stands.
        let [avatar, wide, creep, far, twin] = [unit(), unit(), unit(), unit(), unit()];
        let circle = Footing::Circle;
        let circles = [
            (avatar, Vec2::new(0.0, 0.0), circle(0.5)),
            (wide, Vec2::new(1.0, 0.0), circle(1.0)),
            (creep, Vec2::new(0.25, 0.0), circle(0.35)),
            (far, Vec2::new(4.0, 0.0), circle(0.5)),
            (twin, Vec2::new(0.0, 0.0), circle(0.5)),
        ];
        let at = |x: f32, z: f32| nearest_over(Vec2::new(x, z), circles.into_iter());
        // At x = 0.2: inside the avatar (0.2 away), the wide unit (0.8) and the creep (0.05); the
        // creep is nearest.
        assert_eq!(at(0.2, 0.0), Some(creep));
        // At x = 0.625: outside the avatar (0.625 > 0.5) and the creep (0.375 > 0.35); inside the
        // wide unit.
        assert_eq!(at(0.625, 0.0), Some(wide));
        // At 0.25 behind the origin: the avatar and its twin, as near; the lower id.
        assert_eq!(at(0.0, -0.25), Some(avatar));
        // On the far avatar's edge, exactly 0.5 away, it holds; a quarter meter aside, 0.559 away,
        // it does not.
        assert_eq!(at(4.5, 0.0), Some(far));
        assert_eq!(at(4.5, 0.25), None);
        assert_eq!(at(3.0, 0.0), None);
        // A box of half sides 2 and 0.5 at (10, 0), turned a quarter: Bevy turns its x axis to
        // (0, -1), so it reaches 2 along z and 0.5 along x. It covers (10.4, 1.9), not (11, 0).
        let building = unit();
        let boxed = Footing::Box {
            half: [2.0, 0.5],
            yaw: FRAC_PI_2,
        };
        let drawings = [(building, Vec2::new(10.0, 0.0), boxed)];
        let at = |x: f32, z: f32| nearest_over(Vec2::new(x, z), drawings.into_iter());
        assert_eq!(at(10.4, 1.9), Some(building));
        assert_eq!(at(11.0, 0.0), None);
    }

    #[test]
    fn a_drag_selects_the_players_own_units_shown_within_its_rectangle() {
        let mut ids = IdAllocator::default();
        let (own, other) = (PlayerSlot::new(0), PlayerSlot::new(1));
        let mut unit = |owner: Option<PlayerSlot>| Pointed {
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
