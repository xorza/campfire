use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Allow, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy::math::primitives::{Annulus, Plane3d};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use campfire_capabilities::{AbilitySlots, AttackState, Dead, Health, Owner, ResourcePool, Team};
use campfire_sim::{EntityIndex, SimTick, Unpredicted};
use lightyear::prelude::Predicted;

use crate::hud::gauge::{Cooling, Gauge, GaugeKind, share};
use crate::hud::ring::Ring;
use crate::view::{CAMERA, Drawn, Glide, Look};

mod gauge;
mod ring;

/// Makes the match readable with plain shapes over the drawings: a health bar over every unit, the
/// own avatar's resource and cooldowns under its own, a ring where each hit lands, and a ring under
/// the unit the own avatar attacks. It reads the sim's components and changes none.
#[derive(Debug)]
pub(crate) struct Hud;

/// The meshes and materials of the gauges and rings.
#[derive(Resource, Debug)]
struct HudPalette {
    quad: Handle<Mesh>,
    ring: Handle<Mesh>,
    back: Handle<StandardMaterial>,
    friend: Handle<StandardMaterial>,
    foe: Handle<StandardMaterial>,
    resource: Handle<StandardMaterial>,
    cooldown: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
}

/// The ring under the unit the own avatar attacks, hidden while it attacks none.
#[derive(Component, Debug)]
struct TargetMark;

/// The drawn units with no gauges yet: each one's team, whether it has health, its ability slots
/// and a resource, and whether it is the player's own.
type Ungauged<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Team,
        Has<Health>,
        Option<&'static AbilitySlots>,
        Has<ResourcePool>,
        Has<Predicted>,
        Has<Owner>,
    ),
    (With<Drawn>, Without<Gauged>, Allow<Unpredicted>),
>;

/// On a unit: its gauges are made.
#[derive(Component, Debug)]
struct Gauged;

/// What the gauges show of each unit.
type Shown<'w, 's> = Query<
    'w,
    's,
    (
        Has<Dead>,
        Option<&'static Health>,
        Option<&'static ResourcePool>,
        Option<&'static AbilitySlots>,
    ),
    Allow<Unpredicted>,
>;

/// The fill of each gauge: a drawn quad that neither a gauge nor a unit's drawing holds.
type Fills<'w, 's> =
    Query<'w, 's, &'static mut Transform, (With<Mesh3d>, Without<Gauge>, Without<Glide>)>;

/// How far over a drawing's top its gauges start.
const ABOVE: f32 = 0.35;

impl Plugin for Hud {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Hud::set_palette);
        app.add_systems(
            Update,
            (
                Hud::add_gauges,
                Hud::mark_hits,
                Hud::fill_gauges,
                Hud::place_gauges,
                Hud::widen_rings,
                Hud::mark_target,
            )
                .chain(),
        );
    }
}

impl Hud {
    fn set_palette(
        mut commands: Commands<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let mut flat = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..StandardMaterial::default()
            })
        };
        let ring = meshes.add(Annulus::new(0.55, 0.7).mesh());
        commands.spawn((
            TargetMark,
            Mesh3d(ring.clone()),
            MeshMaterial3d(flat(Color::srgb(1.0, 0.35, 0.2))),
            Transform::default(),
            Visibility::Hidden,
        ));
        commands.insert_resource(HudPalette {
            quad: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
            ring,
            back: flat(Color::srgb(0.1, 0.1, 0.12)),
            friend: flat(Color::srgb(0.3, 0.85, 0.35)),
            foe: flat(Color::srgb(0.9, 0.3, 0.25)),
            resource: flat(Color::srgb(0.3, 0.55, 1.0)),
            cooldown: flat(Color::srgb(0.9, 0.9, 0.9)),
            hit: flat(Color::srgb(1.0, 0.8, 0.3)),
        });
    }

    /// Gives each drawn unit its gauges, once the client holds its own avatar, whose team tells
    /// friend from foe: health for every unit with health; resource and one per ability slot for
    /// the own avatar.
    fn add_gauges(
        palette: Res<'_, HudPalette>,
        own: Query<'_, '_, &Team, (With<Owner>, With<Predicted>)>,
        drawn: Ungauged<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok(&own_team) = own.single() else {
            return;
        };
        for (unit, team, health, slots, resource, predicted, owned) in &drawn {
            commands.entity(unit).insert(Gauged);
            let mine = predicted && owned;
            let friend = *team == own_team;
            let mut kinds = Vec::new();
            if health {
                kinds.push((
                    GaugeKind::Health { shown: None },
                    if friend {
                        &palette.friend
                    } else {
                        &palette.foe
                    },
                ));
            }
            if mine && resource {
                kinds.push((GaugeKind::Resource, &palette.resource));
            }
            if mine && let Some(slots) = slots {
                for (slot, _) in (0..).zip(slots.iter()) {
                    kinds.push((
                        GaugeKind::Cooldown {
                            slot,
                            cooling: None,
                        },
                        &palette.cooldown,
                    ));
                }
            }
            for (kind, fill) in kinds {
                let layout = kind.layout();
                let back = commands
                    .spawn((
                        Mesh3d(palette.quad.clone()),
                        MeshMaterial3d(palette.back.clone()),
                        layout.back(),
                    ))
                    .id();
                let fill = commands
                    .spawn((
                        Mesh3d(palette.quad.clone()),
                        MeshMaterial3d(fill.clone()),
                        layout.fill(0.0),
                    ))
                    .id();
                commands
                    .spawn((
                        Gauge { unit, kind, fill },
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .add_children(&[back, fill]);
            }
        }
    }

    /// Puts a ring on the ground where a unit's health dropped since its gauge last showed it.
    fn mark_hits(
        time: Res<'_, Time>,
        palette: Res<'_, HudPalette>,
        units: Query<'_, '_, (&Health, &Drawn), Allow<Unpredicted>>,
        drawings: Query<'_, '_, (&Transform, &Glide)>,
        mut gauges: Query<'_, '_, &mut Gauge>,
        mut commands: Commands<'_, '_>,
    ) {
        for mut gauge in &mut gauges {
            let unit = gauge.unit;
            let GaugeKind::Health { shown } = &mut gauge.kind else {
                continue;
            };
            let Ok((health, drawn)) = units.get(unit) else {
                continue;
            };
            let current = health.current();
            let hit = shown.is_some_and(|shown| current < shown);
            *shown = Some(current);
            let Ok((transform, glide)) = drawings.get(drawn.drawing()) else {
                continue;
            };
            if hit {
                commands.spawn((
                    Mesh3d(palette.ring.clone()),
                    MeshMaterial3d(palette.hit.clone()),
                    Transform::from_translation(glide.ground(transform) + Vec3::Y * 0.05)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                    Ring {
                        since: time.elapsed_secs(),
                    },
                ));
            }
        }
    }

    /// Sets each gauge's fill from its unit, and hides the gauges of the dead and of unlearned
    /// abilities.
    fn fill_gauges(
        tick: Option<Res<'_, SimTick>>,
        units: Shown<'_, '_>,
        mut gauges: Query<'_, '_, (&mut Gauge, &mut Visibility)>,
        mut fills: Fills<'_, '_>,
    ) {
        let now = tick.map(|tick| tick.start());
        for (mut gauge, mut visibility) in &mut gauges {
            let Ok((dead, health, pool, slots)) = units.get(gauge.unit) else {
                continue;
            };
            let fraction = match &mut gauge.kind {
                GaugeKind::Health { .. } => {
                    health.map(|health| share(health.current(), health.max()))
                }
                GaugeKind::Resource => pool.map(|pool| share(pool.current(), pool.max())),
                GaugeKind::Cooldown { slot, cooling } => {
                    let learned = slots
                        .and_then(|slots| slots.slot(*slot))
                        .filter(|state| state.rank > 0);
                    learned.zip(now).map(|(state, now)| {
                        if cooling.is_none_or(|cooling| cooling.ready_at != state.ready_at) {
                            *cooling = Some(Cooling {
                                ready_at: state.ready_at,
                                seen_at: now,
                            });
                        }
                        cooling.map_or(1.0, |cooling| cooling.filled(now))
                    })
                }
            };
            let shown = fraction.filter(|_| !dead);
            *visibility = if shown.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let (Some(fraction), Ok(mut fill)) = (shown, fills.get_mut(gauge.fill)) {
                *fill = gauge.kind.layout().fill(fraction);
            }
        }
    }

    /// Puts each gauge over its unit's drawing, facing the camera, and removes the gauges of units
    /// that are gone.
    fn place_gauges(
        units: Query<'_, '_, &Drawn, Allow<Unpredicted>>,
        drawings: Query<'_, '_, (&Transform, &Glide, &Look)>,
        mut gauges: Query<'_, '_, (Entity, &Gauge, &mut Transform), Without<Glide>>,
        mut commands: Commands<'_, '_>,
    ) {
        let facing = Quat::from_rotation_arc(Vec3::Y, CAMERA.normalize());
        for (entity, gauge, mut transform) in &mut gauges {
            let drawing = units
                .get(gauge.unit)
                .ok()
                .and_then(|drawn| drawings.get(drawn.drawing()).ok());
            let Some((drawing, glide, look)) = drawing else {
                commands.entity(entity).despawn();
                continue;
            };
            let top = glide.ground(drawing) + Vec3::Y * (look.height() + ABOVE);
            *transform = gauge.kind.layout().place(top, facing);
        }
    }

    /// Puts the target ring on the ground under the unit the own avatar attacks, as wide as the
    /// unit's drawing, or hides it.
    fn mark_target(
        index: Res<'_, EntityIndex>,
        own: Query<'_, '_, &AttackState, With<Predicted>>,
        units: Query<'_, '_, &Drawn, Allow<Unpredicted>>,
        drawings: Query<'_, '_, (&Transform, &Glide, &Look), Without<TargetMark>>,
        mark: Single<'_, '_, (&mut Transform, &mut Visibility), With<TargetMark>>,
    ) {
        let target = own
            .iter()
            .find_map(|attack| attack.target())
            .and_then(|target| index.get(target))
            .and_then(|target| units.get(target).ok())
            .and_then(|drawn| drawings.get(drawn.drawing()).ok());
        let (mut transform, mut visibility) = mark.into_inner();
        let Some((drawing, glide, look)) = target else {
            *visibility = Visibility::Hidden;
            return;
        };
        *visibility = Visibility::Visible;
        *transform = Transform::from_translation(glide.ground(drawing) + Vec3::Y * 0.04)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
            .with_scale(Vec3::splat(2.0 * look.radius()));
    }

    fn widen_rings(
        time: Res<'_, Time>,
        mut rings: Query<'_, '_, (Entity, &Ring, &mut Transform)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, ring, mut transform) in &mut rings {
            match ring.scale(time.elapsed_secs()) {
                Some(scale) => transform.scale = Vec3::splat(scale),
                None => commands.entity(entity).despawn(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::time::TimePlugin;

    use super::*;

    #[test]
    fn the_hud_systems_run_together() {
        // Bevy checks that a system's queries do not conflict when it first runs.
        let mut app = App::new();
        app.add_plugins((TimePlugin, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_resource::<EntityIndex>();
        app.add_plugins(Hud);
        app.update();
        assert!(app.world().contains_resource::<HudPalette>());
        // With no own avatar there is no target to mark.
        let mut marks = app
            .world_mut()
            .query_filtered::<&Visibility, With<TargetMark>>();
        let marks: Vec<_> = marks.iter(app.world()).collect();
        assert_eq!(marks, [&Visibility::Hidden]);
    }
}
