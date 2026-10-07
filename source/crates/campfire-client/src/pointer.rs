use std::fmt;

use bevy::camera::Camera;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::system::{Query, Single, SystemParam};
use bevy::math::primitives::InfinitePlane3d;
use bevy::math::{Vec2, Vec3};
use bevy::transform::components::{GlobalTransform, Transform};
use bevy::window::{PrimaryWindow, Window};
use campfire_capabilities::{Dead, Owner, Team};
use campfire_sim::{StableId, Unpredicted};
use lightyear::prelude::Predicted;

use crate::view::{Drawn, Footing, Look};

/// What the cursor points at, as the player sees the match: the ground point under it, and the
/// living unit drawn there.
#[derive(SystemParam)]
pub(crate) struct Pointer<'w, 's> {
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    own: OwnAvatar<'w, 's>,
    units: LivingUnits<'w, 's>,
    drawings: Query<'w, 's, (&'static Transform, &'static Look)>,
}

/// The player's own avatar: the unit it predicts under a player's control.
type OwnAvatar<'w, 's> =
    Query<'w, 's, (&'static StableId, &'static Team), (With<Owner>, With<Predicted>)>;

/// The living units the client holds, and the entity each is drawn by.
type LivingUnits<'w, 's> = Query<
    'w,
    's,
    (&'static StableId, &'static Team, &'static Drawn),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// A unit the player may point at: its stable id and its team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pointed {
    pub(crate) id: StableId,
    pub(crate) team: Team,
}

impl Pointer<'_, '_> {
    /// The player's own avatar, once the client holds it.
    pub(crate) fn own_avatar(&self) -> Option<Pointed> {
        let (&id, &team) = self.own.single().ok()?;
        Some(Pointed { id, team })
    }

    /// The point of the ground under the cursor, when the cursor is over the window and the ground.
    pub(crate) fn ground(&self) -> Option<Vec3> {
        let (camera, place) = *self.camera;
        let cursor = self.window.cursor_position()?;
        let ray = camera.viewport_to_world(place, cursor).ok()?;
        ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
    }

    /// The living unit, other than the player's own avatar, drawn over `point` of the ground.
    pub(crate) fn unit_at(&self, point: Vec3) -> Option<Pointed> {
        let own = self.own_avatar().map(|own| own.id);
        let drawn = self
            .units
            .iter()
            .filter(|&(&id, _, _)| Some(id) != own)
            .filter_map(|(&id, &team, drawn)| {
                let (transform, look) = self.drawings.get(drawn.drawing()).ok()?;
                let center = Vec2::new(transform.translation.x, transform.translation.z);
                Some((Pointed { id, team }, center, look.footing()))
            });
        nearest_over(Vec2::new(point.x, point.z), drawn)
    }
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
}
