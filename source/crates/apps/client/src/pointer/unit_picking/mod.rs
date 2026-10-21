use bevy::app::{App, Plugin, PreUpdate};
use bevy::camera::{Camera, Camera3d, Projection};
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Local, Query, Res, ResMut};
use bevy::math::Vec3;
use bevy::picking::PickingSystems;
use bevy::picking::backend::ray::RayMap;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::pointer::PointerId;
use bevy::shape::Ray3d;
use bevy::transform::components::GlobalTransform;

use crate::pointer::{LivingUnits, Pointed};
use crate::view::drawing::DrawingOf;
use crate::view::look::UnitShape;
use crate::view::pixel_scale::PixelScale;

/// The units' picking backend: each frame, the units the ray of each pointer through the match's
/// camera meets, by their shapes, sent to Bevy's picking as hits of their drawings' roots, ranked
/// by tier, then depth, then stable id (12-unit-picking.md, Picking is a Bevy picking backend).
#[derive(Debug)]
pub(crate) struct UnitPickingPlugin;

/// The living units the mouse's ray meets this frame, ranked as the hover ranks them, the first
/// on top (12-unit-picking.md, One ranked list for the client's queries).
#[derive(Resource, Debug, Default)]
pub(crate) struct UnderMouse(Vec<Pointed>);

/// How the units are picked: one tier, and no least size on the screen, the shapes alone.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitPicking {
    least_radius_px: f32,
}

/// A unit a pointer's ray meets: its drawing's root, the unit, its tier, the lower on top, and
/// how far along the ray it was met, in meters.
#[derive(Debug, Clone, Copy)]
struct UnitHit {
    root: Entity,
    unit: Pointed,
    tier: u32,
    depth: f32,
}

/// A unit's shape where it stands, as a pointer's ray tests it: its drawing's root, the unit, its
/// tier, the point its shape stands on, and the shape.
#[derive(Debug, Clone, Copy)]
struct Target {
    root: Entity,
    unit: Pointed,
    tier: u32,
    base: Vec3,
    shape: UnitShape,
}

/// The drawing roots that have a shape, each with where it stands and its unit.
type ShapedRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DrawingOf,
        &'static GlobalTransform,
        &'static UnitShape,
    ),
>;

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
            .add_systems(PreUpdate, Self::pick.in_set(PickingSystems::Backend));
    }
}

impl UnitPickingPlugin {
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
        roots: ShapedRoots<'_, '_>,
        units: LivingUnits<'_, '_>,
        picking: Res<'_, UnitPicking>,
        mut under: ResMut<'_, UnderMouse>,
        mut sent: MessageWriter<'_, PointerHits>,
        mut targets: Local<'_, Vec<Target>>,
        mut hits: Local<'_, Vec<UnitHit>>,
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
        targets.clear();
        targets.extend(
            roots
                .iter()
                .filter_map(|(root, &DrawingOf(unit), at, &shape)| {
                    let (&id, &team, owner, _) = units.get(unit).ok()?;
                    let unit = Pointed::new(id, team, owner);
                    Some(Target {
                        root,
                        unit,
                        tier: picking.tier(unit),
                        base: at.translation(),
                        shape,
                    })
                }),
        );
        let tiers = picking.tiers();
        for (ray_id, &ray) in rays.iter() {
            if ray_id.camera != camera {
                continue;
            }
            picking.hits(ray, &scale, &targets, &mut hits);
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
    /// The tier of `unit`, 0 on top.
    const fn tier(&self, _unit: Pointed) -> u32 {
        0
    }

    /// How many tiers it ranks units in.
    const fn tiers(&self) -> u32 {
        1
    }

    /// Writes to `out` each of `targets` that `ray` meets, each shape widened to the least radius
    /// turned into meters at its center's depth as `scale` shows it, ranked by tier, then depth,
    /// then stable id.
    fn hits(&self, ray: Ray3d, scale: &PixelScale, targets: &[Target], out: &mut Vec<UnitHit>) {
        out.clear();
        out.extend(targets.iter().filter_map(|target| {
            let center = target.base + Vec3::Y * (target.shape.height / 2.0);
            let meters = scale.meters_at(center).unwrap_or(0.0);
            let depth = (target.shape).entry(target.base, ray, self.least_radius_px * meters)?;
            Some(UnitHit {
                root: target.root,
                unit: target.unit,
                tier: target.tier,
                depth,
            })
        }));
        out.sort_unstable_by(|a, b| {
            (a.tier.cmp(&b.tier))
                .then(a.depth.total_cmp(&b.depth))
                .then(a.unit.id.cmp(&b.unit.id))
        });
    }
}

/// The `order` of the hits of `tier` of `tiers`, for a camera of `order`: evenly between the
/// camera's, which a backend of its world sends, and its UI's half above, the first tier highest.
fn tier_order(order: isize, tier: u32, tiers: u32) -> f32 {
    debug_assert!(tier < tiers, "a tier of the ranking");
    order as f32 + 0.5 * (tiers - tier) as f32 / (tiers + 1) as f32
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
