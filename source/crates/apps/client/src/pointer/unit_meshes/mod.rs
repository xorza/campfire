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
mod tests;
