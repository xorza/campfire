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

/// A mesh that draws a unit: the turn from the world into its own frame, its box there, round
/// `center` of `half` sides, its mesh, and its material when it has a file material.
#[derive(Debug, Clone, Copy)]
pub(super) struct Part {
    pub(super) to_local: Affine3A,
    pub(super) center: Vec3,
    pub(super) half: Vec3,
    pub(super) mesh: AssetId<Mesh>,
    pub(super) material: Option<AssetId<FileMaterial>>,
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
                let from = part.to_local.transform_point3(ray.origin);
                let way = part.to_local.transform_vector3(*ray.direction);
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
