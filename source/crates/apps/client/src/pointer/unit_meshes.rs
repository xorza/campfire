use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::InheritedVisibility;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::{Has, With};
use bevy::ecs::system::{Query, SystemParam};
use bevy::math::Vec3;
use bevy::mesh::Mesh3d;
use bevy::shape::Ray3d;
use bevy::transform::components::GlobalTransform;

use crate::pointer::{LivingUnits, Pointed};
use crate::view::drawing::{DrawingOf, Drawn};

/// The meshes that draw the living units, which the pointer picks a unit by, each by its own box,
/// as Zero Hour's `pickDrawable` casts the ray through the cursor at its render objects.
#[derive(SystemParam, Debug)]
pub(super) struct UnitMeshes<'w, 's> {
    meshes: Meshes<'w, 's>,
    above: Query<
        'w,
        's,
        (
            Option<&'static ChildOf>,
            Has<Drawn>,
            Option<&'static DrawingOf>,
        ),
    >,
    units: LivingUnits<'w, 's>,
}

/// The meshes the client draws: each one's entity, its box in its own frame, where it stands in
/// the world, and whether it shows.
type Meshes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Aabb,
        &'static GlobalTransform,
        &'static InheritedVisibility,
    ),
    With<Mesh3d>,
>;

impl UnitMeshes<'_, '_> {
    /// Of the living units, the one that `keep` keeps whose shown mesh `ray` meets first; the
    /// lower stable id on a tie.
    pub(super) fn nearest(&self, ray: Ray3d, keep: impl Fn(Pointed) -> bool) -> Option<Pointed> {
        let hits = self.meshes.iter().filter_map(|(mesh, aabb, place, shown)| {
            if !shown.get() {
                return None;
            }
            let entry = mesh_entry(ray, aabb, place)?;
            let unit = self.drawn_unit(mesh).filter(|&unit| keep(unit))?;
            Some((unit, entry))
        });
        nearest_hit(hits)
    }

    /// The living unit `mesh` draws: one whose mesh is, or hangs under, a `Drawn` child of its
    /// drawing's root, as a gauge or a ring under the root draws none.
    fn drawn_unit(&self, mesh: Entity) -> Option<Pointed> {
        let mut at = mesh;
        let mut drawn = false;
        loop {
            let (child_of, is_drawn, root) = self.above.get(at).ok()?;
            if let Some(&DrawingOf(unit)) = root {
                let (&id, &team, owner, _) = self.units.get(unit).ok().filter(|_| drawn)?;
                return Some(Pointed::new(id, team, owner));
            }
            drawn |= is_drawn;
            at = child_of?.parent();
        }
    }
}

/// Of `hits`, each a unit and how far along the ray one of its meshes was met, the unit met
/// first; the lower stable id on a tie.
fn nearest_hit(hits: impl Iterator<Item = (Pointed, f32)>) -> Option<Pointed> {
    hits.min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)))
        .map(|(unit, _)| unit)
}

/// How far along `ray` it first meets the box `aabb` of a mesh standing at `place`, in the
/// world's meters: the ray taken into the mesh's own frame, where the box is, as the box turns
/// and scales with the mesh, keeps the world's measure along it; none when it misses, or meets it
/// only behind the ray's origin, and 0 from inside it.
fn mesh_entry(ray: Ray3d, aabb: &Aabb, place: &GlobalTransform) -> Option<f32> {
    let to_local = place.affine().inverse();
    let origin = to_local.transform_point3(ray.origin);
    let way = to_local.transform_vector3(*ray.direction);
    let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    let (mut near, mut far) = (0.0_f32, f32::INFINITY);
    for axis in 0..3 {
        let (from, step) = (origin[axis] - center[axis], way[axis]);
        if step == 0.0 {
            // Parallel to the slab: within it or never.
            if from.abs() > half[axis] {
                return None;
            }
            continue;
        }
        let [a, b] = [(-half[axis] - from) / step, (half[axis] - from) / step];
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far).then_some(near)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::ecs::system::SystemState;
    use bevy::ecs::world::World;
    use bevy::math::{Dir3, Quat};
    use bevy::transform::components::Transform;
    use campfire_common::Seat;
    use campfire_sim::{EntityIndex, IdAllocator, StableId};
    use campfire_units::{Dead, Owner};
    use campfire_values::Team;

    use super::*;

    fn ray(origin: Vec3, direction: Vec3) -> Ray3d {
        Ray3d::new(origin, Dir3::new(direction).unwrap())
    }

    fn aabb(center: Vec3, half: Vec3) -> Aabb {
        Aabb {
            center: center.into(),
            half_extents: half.into(),
        }
    }

    #[test]
    fn a_ray_meets_a_meshs_box_as_the_mesh_turns_and_scales_it_and_the_first_met_is_pointed_at() {
        // A box of half sides 0.5, 1 and 0.5 standing on the origin: a level ray at 1 m from
        // z = 10 along −z meets its side at z = 0.5, 9.5 along; over its top, at 2.5 m, none; one
        // that starts inside it meets it where it starts; one that points away, none.
        let post = aabb(Vec3::Y, Vec3::new(0.5, 1.0, 0.5));
        let here = GlobalTransform::IDENTITY;
        let level = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z);
        assert_eq!(mesh_entry(level, &post, &here), Some(9.5));
        let over = ray(Vec3::new(0.0, 2.5, 10.0), Vec3::NEG_Z);
        assert_eq!(mesh_entry(over, &post, &here), None);
        let inside = ray(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_Z);
        assert_eq!(mesh_entry(inside, &post, &here), Some(0.0));
        let away = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::Z);
        assert_eq!(mesh_entry(away, &post, &here), None);
        // The box of half sides 2, 0.5 and 0.25 around (0, 0.5, 0), of a mesh scaled twice,
        // turned a quarter about y and standing at (0, 0, −3): its x half side, 4 m, lies along
        // −z, so it spans z −7 to 1, x −0.5 to 0.5 and y 0 to 2. A ray along x at 1 m and
        // z = 0.9 meets it at x = −0.5, 9.5 along from x = −10; at z = 1.1, none. Unturned, it
        // would span x −4 to 4, and unscaled z −5 to −1.
        let turned = GlobalTransform::from(
            Transform::from_xyz(0.0, 0.0, -3.0)
                .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
                .with_scale(Vec3::splat(2.0)),
        );
        let hull = aabb(Vec3::new(0.0, 0.5, 0.0), Vec3::new(2.0, 0.5, 0.25));
        let along = |z| mesh_entry(ray(Vec3::new(-10.0, 1.0, z), Vec3::X), &hull, &turned);
        let met = along(0.9).unwrap();
        // The quarter turn's cosine rounds to −4.4e-8 in f32, not 0, which moves the entry by
        // that times the ray's 10 m, far below 1e-5.
        assert!((met - 9.5).abs() < 1e-5, "{met}");
        assert_eq!(along(1.1), None);

        // Of the units met, the first met is pointed at, the lower id on a tie.
        let mut ids = IdAllocator::default();
        let mut unit = || Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner: None,
        };
        let [first, second, third] = [unit(), unit(), unit()];
        let hits = [(second, 9.5), (third, 4.0), (first, 4.0)];
        assert_eq!(nearest_hit(hits.into_iter()), Some(first));
        assert_eq!(nearest_hit([(second, 9.5)].into_iter()), Some(second));
        assert_eq!(nearest_hit(std::iter::empty()), None);
    }

    #[test]
    fn a_unit_is_picked_by_the_shown_meshes_it_draws_and_not_by_its_gauges() {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        let mut ids = IdAllocator::default();
        let cube = aabb(Vec3::Y, Vec3::splat(0.5));
        let mut unit = |world: &mut World| -> (Entity, StableId, Entity) {
            let id = ids.allocate();
            let own = Owner::new(Seat::new(0));
            let unit = world.spawn((id, Team::new(0), own)).id();
            let root = world.spawn(DrawingOf(unit)).id();
            (unit, id, root)
        };
        let mesh = |world: &mut World, parent: Entity, z: f32, shown: bool| {
            let shown = if shown {
                InheritedVisibility::VISIBLE
            } else {
                InheritedVisibility::HIDDEN
            };
            let place = GlobalTransform::from_xyz(0.0, 0.0, z);
            let parts = (Mesh3d::default(), cube, place, shown, ChildOf(parent));
            world.spawn(parts).id()
        };
        // The ray runs along −z at 1 m from z = 10, through cubes of half side 0.5 at z = 8, 5,
        // 0 and −5. The first unit's model draws it at z = 0, its mesh a child of the model, met
        // 9.5 along. The second unit's figure draws it at z = −5, met 14.5 along; its gauge,
        // under its root and not drawn, stands at z = 5. The third's figure at z = 8 is hidden.
        let (first, first_id, root) = unit(&mut world);
        let model = world.spawn((Drawn, ChildOf(root))).id();
        mesh(&mut world, model, 0.0, true);
        let (_, second_id, root) = unit(&mut world);
        let figure = mesh(&mut world, root, -5.0, true);
        world.entity_mut(figure).insert(Drawn);
        mesh(&mut world, root, 5.0, true);
        let (_, _, root) = unit(&mut world);
        let hidden = mesh(&mut world, root, 8.0, false);
        world.entity_mut(hidden).insert(Drawn);
        let level = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z);
        let mut meshes = SystemState::<UnitMeshes<'_, '_>>::new(&mut world);
        let picked = |world: &World, meshes: &mut SystemState<UnitMeshes<'_, '_>>, but| {
            let meshes = meshes.get(world).unwrap();
            let unit = meshes.nearest(level, |unit| unit.id != but)?;
            Some(unit.id)
        };
        // The first unit is picked; kept out, or dead, the second.
        let none = ids.allocate();
        assert_eq!(picked(&world, &mut meshes, none), Some(first_id));
        assert_eq!(picked(&world, &mut meshes, first_id), Some(second_id));
        world.entity_mut(first).insert(Dead);
        assert_eq!(picked(&world, &mut meshes, none), Some(second_id));
        assert_eq!(picked(&world, &mut meshes, second_id), None);
    }
}
