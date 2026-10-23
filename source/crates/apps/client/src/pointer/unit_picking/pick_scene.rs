use std::fmt;
use std::ops::Range;

use bevy::asset::{AssetId, Assets};
use bevy::ecs::entity::Entity;
use bevy::math::{Affine3A, Vec3};
use bevy::mesh::Mesh;
use bevy::shape::Ray3d;

use crate::pointer::Pointed;
use crate::pointer::unit_picking::mesh_ray;
use crate::view::file_material::FileMaterial;
use crate::view::look::UnitShape;

/// The units a pointer's ray may meet this frame, as the backend gathers them once for every
/// pointer: each unit, and the meshes that draw it, one run of them for each unit.
#[derive(Debug, Default)]
pub(super) struct PickScene {
    targets: Vec<Placed>,
    parts: Vec<Part>,
}

/// A unit as a ray tests it: its drawing's root, the unit, its tier, the lower on top, the point
/// its shape stands on, its shape, and the least radius in meters it keeps at its depth.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target {
    pub(super) root: Entity,
    pub(super) unit: Pointed,
    pub(super) tier: u32,
    pub(super) base: Vec3,
    pub(super) shape: UnitShape,
    pub(super) least: f32,
}

/// A mesh that draws a unit: its place in the world, its box in its own frame, round `center` of
/// `half` sides, a sphere in the world that holds that box, its mesh, and its material when it
/// has a file material. A ray tests the sphere first, so only a part it may meet turns the ray
/// into its own frame.
#[derive(Debug, Clone, Copy)]
pub(super) struct Part {
    place: Affine3A,
    center: Vec3,
    half: Vec3,
    world_center: Vec3,
    radius: f32,
    mesh: AssetId<Mesh>,
    material: Option<AssetId<FileMaterial>>,
}

/// A unit and the run of the scene's parts that draw it.
#[derive(Debug, Clone)]
struct Placed {
    target: Target,
    parts: Range<usize>,
}

/// What a part's ray test reads: the meshes' triangles, and the materials, which may let a ray
/// through.
#[derive(Clone, Copy)]
pub(super) struct PartAssets<'a> {
    pub(super) meshes: &'a Assets<Mesh>,
    pub(super) materials: &'a Assets<FileMaterial>,
}

/// A unit a pointer's ray meets: its drawing's root, the unit, its tier, the lower on top, and
/// how far along the ray it was met, in meters.
#[derive(Debug, Clone, Copy)]
pub(super) struct UnitHit {
    pub(super) root: Entity,
    pub(super) unit: Pointed,
    pub(super) tier: u32,
    pub(super) depth: f32,
}

/// The asset stores print nothing of use.
impl fmt::Debug for PartAssets<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PartAssets")
    }
}

/// What a part is made of: its place in the world, its box in its own frame, its mesh and its
/// material when it has a file material.
#[derive(Debug, Clone, Copy)]
pub(super) struct PartSetup {
    pub(super) place: Affine3A,
    pub(super) center: Vec3,
    pub(super) half: Vec3,
    pub(super) mesh: AssetId<Mesh>,
    pub(super) material: Option<AssetId<FileMaterial>>,
}

/// The part of a setup: the sphere's radius scales the box's by the transform's Frobenius norm,
/// which no stretch of it passes, shear and all.
impl From<PartSetup> for Part {
    fn from(setup: PartSetup) -> Self {
        let PartSetup {
            place,
            center,
            half,
            mesh,
            material,
        } = setup;
        let axes = place.matrix3;
        let stretch = (axes.x_axis.length_squared()
            + axes.y_axis.length_squared()
            + axes.z_axis.length_squared())
        .sqrt();
        Self {
            place,
            center,
            half,
            world_center: place.transform_point3(center),
            radius: half.length() * stretch,
            mesh,
            material,
        }
    }
}

impl Part {
    /// How far along `ray` it enters the part's sphere, 0 from inside it; none when it misses.
    fn sphere_entry(&self, ray: Ray3d) -> Option<f32> {
        let to_center = self.world_center - ray.origin;
        let along = to_center.dot(*ray.direction);
        let apart = to_center.length_squared() - along * along;
        let reach = self.radius * self.radius;
        if apart > reach {
            return None;
        }
        let half_chord = (reach - apart).sqrt();
        (along + half_chord >= 0.0).then(|| (along - half_chord).max(0.0))
    }
}

impl PickScene {
    /// Empties it for a new frame.
    pub(super) fn clear(&mut self) {
        self.targets.clear();
        self.parts.clear();
    }

    /// Adds `target`, drawn by `parts`.
    pub(super) fn push(&mut self, target: Target, parts: impl IntoIterator<Item = Part>) {
        let start = self.parts.len();
        self.parts.extend(parts);
        self.targets.push(Placed {
            target,
            parts: start..self.parts.len(),
        });
    }

    /// Writes to `out` each unit that `ray` meets, ranked by tier, then depth, then stable id: the
    /// nearest of its parts' triangles but those of a material that lets rays through, or the
    /// cylinder of its least radius round its axis (02-engine-core.md, A unit is picked by its
    /// drawn triangles; 02-engine-core.md, A least size on screen). A part's box, which holds its
    /// triangles, spares their test when it lies no nearer than the unit's nearest hit yet.
    pub(super) fn hits(&self, ray: Ray3d, assets: PartAssets<'_>, out: &mut Vec<UnitHit>) {
        out.clear();
        for Placed { target, parts } in &self.targets {
            let mut depth = (target.least > 0.0)
                .then(|| target.shape.axis_entry(target.base, ray, target.least))
                .flatten();
            for part in &self.parts[parts.clone()] {
                let Some(near) = part.sphere_entry(ray) else {
                    continue;
                };
                if depth.is_some_and(|depth| near >= depth) {
                    continue;
                }
                let to_local = part.place.inverse();
                let from = to_local.transform_point3(ray.origin);
                let way = to_local.transform_vector3(*ray.direction);
                let Some(entry) = mesh_ray::box_entry(from, way, part.center, part.half) else {
                    continue;
                };
                if depth.is_some_and(|depth| entry >= depth) {
                    continue;
                }
                let material = part.material.and_then(|id| assets.materials.get(id));
                if material.is_some_and(|material| material.extension.lets_rays_through()) {
                    continue;
                }
                let Some(mesh) = assets.meshes.get(part.mesh) else {
                    continue;
                };
                if let Some(met) = mesh_ray::triangles_entry(from, way, mesh) {
                    depth = Some(depth.map_or(met, |depth| depth.min(met)));
                }
            }
            if let Some(depth) = depth {
                out.push(UnitHit {
                    root: target.root,
                    unit: target.unit,
                    tier: target.tier,
                    depth,
                });
            }
        }
        out.sort_unstable_by(|a, b| {
            (a.tier.cmp(&b.tier))
                .then(a.depth.total_cmp(&b.depth))
                .then(a.unit.id.cmp(&b.unit.id))
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Dir3;

    use super::*;

    #[test]
    fn a_ray_enters_a_parts_sphere_at_its_near_side_and_misses_one_behind_or_aside() {
        // A box of half sides (1, 0, 0) round (0, 0, −5) under no turn: its sphere's radius is the
        // half sides' length, 1, times the identity's norm, √3, so a ray from the origin toward
        // −z enters it at 5 − √3, within the rounding of an `f32` square root.
        let part = Part::from(PartSetup {
            place: Affine3A::IDENTITY,
            center: Vec3::new(0.0, 0.0, -5.0),
            half: Vec3::X,
            mesh: AssetId::default(),
            material: None,
        });
        let ray = |origin: Vec3, direction: Dir3| Ray3d::new(origin, direction);
        let entry = part.sphere_entry(ray(Vec3::ZERO, Dir3::NEG_Z)).unwrap();
        assert!((entry - (5.0 - 3.0_f32.sqrt())).abs() < 1e-6, "{entry}");
        // The sphere behind the ray, or 3 m aside of it, past √3, is missed; from inside, the
        // ray is in it at once.
        assert_eq!(part.sphere_entry(ray(Vec3::ZERO, Dir3::Z)), None);
        assert_eq!(part.sphere_entry(ray(Vec3::X * 3.0, Dir3::NEG_Z)), None);
        let inside = Vec3::new(0.0, 0.0, -5.0);
        assert_eq!(part.sphere_entry(ray(inside, Dir3::Z)), Some(0.0));
    }
}
