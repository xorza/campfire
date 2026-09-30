use bevy::camera::Camera;
use bevy::ecs::query::{Allow, With, Without};
use bevy::ecs::system::{Query, Single, SystemParam};
use bevy::math::primitives::InfinitePlane3d;
use bevy::math::{Vec2, Vec3};
use bevy::transform::components::{GlobalTransform, Transform};
use bevy::window::{PrimaryWindow, Window};
use campfire_capabilities::{Dead, Owner, Team};
use campfire_net::Unpredicted;
use campfire_sim::StableId;
use lightyear::prelude::Predicted;

use crate::view::{Drawn, Look};

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
    pub(crate) fn own_hero(&self) -> Option<Pointed> {
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
        let own = self.own_hero().map(|own| own.id);
        let drawn = self
            .units
            .iter()
            .filter(|&(&id, _, _)| Some(id) != own)
            .filter_map(|(&id, &team, drawn)| {
                let (transform, look) = self.drawings.get(drawn.drawing()).ok()?;
                let center = Vec2::new(transform.translation.x, transform.translation.z);
                Some((Pointed { id, team }, center, look.radius()))
            });
        nearest_over(Vec2::new(point.x, point.z), drawn)
    }
}

/// Of the circles on the ground, each a unit's center and radius, the unit whose circle holds
/// `point` and whose center is nearest it; the lower stable id on a tie.
fn nearest_over(
    point: Vec2,
    circles: impl Iterator<Item = (Pointed, Vec2, f32)>,
) -> Option<Pointed> {
    circles
        .map(|(unit, center, radius)| (unit, point.distance_squared(center), radius))
        .filter(|&(_, distance, radius)| distance <= radius * radius)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)))
        .map(|(unit, _, _)| unit)
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn the_nearest_unit_whose_drawing_holds_the_point_is_the_one_pointed_at() {
        let mut ids = IdAllocator::default();
        let mut unit = || Pointed {
            id: ids.allocate(),
            team: Team::new(0),
        };
        // An avatar of 0.5 m at the origin, a unit of 1 m at x = 1, a creep of 0.35 m at x = 0.25, a
        // avatar at x = 4, and a unit of a higher id drawn where the first stands.
        let [avatar, wide, creep, far, twin] = [unit(), unit(), unit(), unit(), unit()];
        let circles = [
            (avatar, Vec2::new(0.0, 0.0), 0.5),
            (wide, Vec2::new(1.0, 0.0), 1.0),
            (creep, Vec2::new(0.25, 0.0), 0.35),
            (far, Vec2::new(4.0, 0.0), 0.5),
            (twin, Vec2::new(0.0, 0.0), 0.5),
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
    }
}
