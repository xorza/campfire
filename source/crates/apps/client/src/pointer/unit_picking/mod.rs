use bevy::app::{App, Plugin, PreUpdate};
use bevy::asset::Assets;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::InheritedVisibility;
use bevy::camera::{Camera, Camera3d, Projection};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::{Added, Allow, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::{resource_added, resource_exists};
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, SystemParam};
use bevy::math::Vec3;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::MeshMaterial3d;
use bevy::picking::PickingSystems;
use bevy::picking::backend::ray::RayMap;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::pointer::PointerId;
use bevy::transform::components::GlobalTransform;
use campfire_client_files::PickingFile;
use campfire_common::ErrorReport;
use campfire_script::{Filter, MatchNames, TagNames, UnknownName};
use campfire_sim::{StableId, Unpredicted};
use campfire_units::{Dead, Owner, TagBook};
use campfire_values::{Relation, Relations, TagSet, Team, UnitType};
use tracing::error;

use crate::own_player::OwnPlayer;
use crate::pointer::Pointed;
use crate::pointer::unit_picking::pick_parts::{PickPartOf, PickParts};
use crate::pointer::unit_picking::pick_scene::{Part, PartAssets, PickScene, Target, UnitHit};
use crate::view::client_data::ClientData;
use crate::view::drawing::{DrawingOf, Drawn};
use crate::view::file_material::FileMaterial;
use crate::view::look::UnitShape;
use crate::view::pixel_scale::PixelScale;

/// The units' picking backend: each frame, the units the ray of each pointer through the match's
/// camera meets, by their shapes, sent to Bevy's picking as hits of their drawings' roots, ranked
/// by tier, then depth, then stable id (02-engine-core.md, Picking is a Bevy picking backend).
#[derive(Debug)]
pub(crate) struct UnitPickingPlugin;

/// The living units the mouse's ray meets this frame, ranked as the hover ranks them, the first
/// on top (02-engine-core.md, One ranked list for the client's queries).
#[derive(Resource, Debug, Default)]
pub(crate) struct UnderMouse(Vec<Pointed>);

/// How the units are picked, as the mode's picking file gives it with its filters resolved
/// against the match's tags: the tiers' filters, a unit in the tier of the first that selects it
/// and one none selects in a last tier, and the least radius on the screen in pixels; one tier
/// and no least size until the match's names come, or for a file that names a tag the match
/// lacks (02-engine-core.md, The mode chooses its picking).
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitPicking {
    filters: Box<[Filter]>,
    least_radius_px: f32,
}

/// The drawing roots that have a shape, each with where it stands, its unit, and the meshes that
/// draw it.
type ShapedRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DrawingOf,
        &'static GlobalTransform,
        &'static UnitShape,
        Option<&'static PickParts>,
    ),
>;

/// The meshes that draw units: each one's mesh and box, where it stands, whether it shows, and
/// its material when it has a file material.
type PartMeshes<'w, 's> = Query<
    'w,
    's,
    (
        &'static Mesh3d,
        &'static Aabb,
        &'static GlobalTransform,
        &'static InheritedVisibility,
        Option<&'static MeshMaterial3d<FileMaterial>>,
    ),
>;

/// The meshes, and the ancestors a new one is told to its unit's drawing by: each one's parent,
/// whether it is a child that draws its unit, and whether it is a drawing's root that has a shape.
type Ancestors<'w, 's> = Query<'w, 's, (Option<&'static ChildOf>, Has<Drawn>, Has<UnitShape>)>;

/// The living units the client holds: each one's stable id, its team, the player who controls
/// it, and its type.
type Living<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Team,
        Option<&'static Owner>,
        &'static UnitType,
    ),
    (Without<Dead>, Allow<Unpredicted>),
>;

/// How the player regards each team: the side its units show as friends on, and the teams'
/// relations on the match's entity.
#[derive(SystemParam, Debug)]
struct Regard<'w, 's> {
    own: OwnPlayer<'w, 's>,
    relations: Query<'w, 's, &'static Relations, Allow<Unpredicted>>,
}

/// The match's camera: its entity, its viewport and order, where it stands, and its projection.
type MatchCamera<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Camera,
        &'static GlobalTransform,
        &'static Projection,
    ),
    With<Camera3d>,
>;

impl Plugin for UnitPickingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UnderMouse>()
            .init_resource::<UnitPicking>()
            .add_systems(
                PreUpdate,
                (
                    Self::resolve.run_if(resource_added::<MatchNames>),
                    Self::mark_parts,
                    Self::pick.run_if(resource_exists::<TagBook>),
                )
                    .chain()
                    .in_set(PickingSystems::Backend),
            );
    }
}

impl UnitPickingPlugin {
    /// Resolves the mode's picking file against the match's tags as they come; a file that names
    /// a tag the match lacks is refused, logged, and picks as a mode of no file.
    fn resolve(
        data: Res<'_, ClientData>,
        names: Res<'_, MatchNames>,
        mut picking: ResMut<'_, UnitPicking>,
    ) {
        *picking = UnitPicking::resolve(&data.picking, names.tags()).unwrap_or_else(|problem| {
            error!(path = %PickingFile::path(), error = %ErrorReport::of(&problem), "a client data file names what the match lacks");
            UnitPicking::default()
        });
    }

    /// Tells each new mesh that draws a unit, one that is, or hangs under, a `Drawn` child of a
    /// drawing's root with a shape, to that root, so a gauge or a ring under the root is no part.
    fn mark_parts(
        meshes: Query<'_, '_, Entity, Added<Mesh3d>>,
        above: Ancestors<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        for mesh in &meshes {
            let mut at = mesh;
            let mut drawn = false;
            while let Ok((child_of, is_drawn, is_root)) = above.get(at) {
                if is_root {
                    if drawn {
                        commands.entity(mesh).insert(PickPartOf(at));
                    }
                    break;
                }
                drawn |= is_drawn;
                let Some(child_of) = child_of else {
                    break;
                };
                at = child_of.parent();
            }
        }
    }

    /// Sends, for each pointer whose ray of the match's camera meets a unit, its hits of each
    /// tier in a `PointerHits` of its own, of an `order` above the camera's and below its UI's, a
    /// higher tier above, and keeps the mouse's ranked units. Bevy's hits own their list, one for
    /// each tier a ray meets.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn pick(
        rays: Res<'_, RayMap>,
        cameras: MatchCamera<'_, '_>,
        (roots, units, parts): (ShapedRoots<'_, '_>, Living<'_, '_>, PartMeshes<'_, '_>),
        (picking, book, regard): (Res<'_, UnitPicking>, Res<'_, TagBook>, Regard<'_, '_>),
        (meshes, materials): (Res<'_, Assets<Mesh>>, Res<'_, Assets<FileMaterial>>),
        mut under: ResMut<'_, UnderMouse>,
        mut sent: MessageWriter<'_, PointerHits>,
        (mut scene, mut hits): (Local<'_, PickScene>, Local<'_, Vec<UnitHit>>),
    ) {
        under.0.clear();
        let Ok((camera, view, place, projection)) = cameras.single() else {
            return;
        };
        let Some(size) = view.logical_viewport_size() else {
            return;
        };
        if !rays.iter().any(|(ray_id, _)| ray_id.camera == camera) {
            return;
        }
        let scale = PixelScale::of(place.compute_transform(), projection, size.y);
        let relations = regard.relations.single().ok();
        let side = regard.own.side();
        let relation = |team| Some(relations?.between(side?, team));
        scene.clear();
        for (root, &DrawingOf(unit), at, &shape, drawn_by) in &roots {
            let Ok((&id, &team, owner, &unit_type)) = units.get(unit) else {
                continue;
            };
            let base = at.translation();
            let target = Target {
                root,
                unit: Pointed::new(id, team, owner),
                tier: picking.tier(relation(team), book.own(unit_type)),
                base,
                shape,
                least: picking.least_at(&scale, base + Vec3::Y * (shape.height / 2.0)),
            };
            let shown = drawn_by
                .into_iter()
                .flat_map(PickParts::iter)
                .filter_map(|part| {
                    let (mesh, aabb, place, shown, material) = parts.get(part).ok()?;
                    shown.get().then(|| Part {
                        to_local: place.affine().inverse(),
                        center: aabb.center.into(),
                        half: aabb.half_extents.into(),
                        mesh: mesh.id(),
                        material: material.map(|material| material.id()),
                    })
                });
            scene.push(target, shown);
        }
        let assets = PartAssets {
            meshes: &meshes,
            materials: &materials,
        };
        let tiers = picking.tiers();
        for (ray_id, &ray) in rays.iter() {
            if ray_id.camera != camera {
                continue;
            }
            scene.hits(ray, assets, &mut hits);
            for tier in hits.chunk_by(|a, b| a.tier == b.tier) {
                let picks = (tier.iter())
                    .map(|hit| (hit.root, HitData::new(camera, hit.depth, None, None)))
                    .collect();
                let order = tier_order(view.order, tier[0].tier, tiers);
                sent.write(PointerHits::new(ray_id.pointer, picks, order));
            }
            if ray_id.pointer == PointerId::Mouse {
                under.0.extend(hits.iter().map(|hit| hit.unit));
            }
        }
    }
}

impl UnderMouse {
    /// The units, the first on top.
    pub(super) fn iter(&self) -> impl Iterator<Item = Pointed> {
        self.0.iter().copied()
    }
}

impl UnitPicking {
    /// The picking `file` gives, its filters resolved against `tags`; an error for a tag the
    /// match lacks.
    fn resolve(file: &PickingFile, tags: &TagNames) -> Result<Self, UnknownName> {
        let filters = (file.tiers.iter())
            .map(|filter| Filter::resolve(filter, tags))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            filters,
            least_radius_px: file.least_radius_px.get(),
        })
    }

    /// The tier of a unit of type tags `tags` of a team the player regards with `relation`, 0 on
    /// top: the first filter's that selects it, or the last tier; with no relation, before the
    /// player has a side, a filter selects it only as it would for every relation.
    fn tier(&self, relation: Option<Relation>, tags: TagSet) -> u32 {
        let selects = |filter: &Filter| match relation {
            Some(relation) => filter.selects(relation, tags),
            None => Relation::ALL.iter().all(|&any| filter.selects(any, tags)),
        };
        let tier = (self.filters.iter().position(selects)).unwrap_or(self.filters.len());
        u32::try_from(tier).expect("a file of few tiers")
    }

    /// How many tiers it ranks units in: one for each filter, and the last.
    fn tiers(&self) -> u32 {
        u32::try_from(self.filters.len() + 1).expect("a file of few tiers")
    }

    /// The least radius in meters at `center`, its pixels as `scale` shows them at its depth;
    /// none behind the camera.
    fn least_at(&self, scale: &PixelScale, center: Vec3) -> f32 {
        self.least_radius_px * scale.meters_at(center).unwrap_or(0.0)
    }
}

/// The `order` of the hits of `tier` of `tiers`, for a camera of `order`: evenly between the
/// camera's, which a backend of its world sends, and its UI's half above, the first tier highest.
fn tier_order(order: isize, tier: u32, tiers: u32) -> f32 {
    debug_assert!(tier < tiers, "a tier of the ranking");
    order as f32 + 0.5 * (tiers - tier) as f32 / (tiers + 1) as f32
}

mod mesh_ray;
mod pick_parts;
mod pick_scene;

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
