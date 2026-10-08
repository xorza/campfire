use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::math::primitives::{Annulus, Plane3d};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use campfire_capabilities::{
    ActionSlots, Combat, Dead, Learning, Level, PlayerUnits, Points, PoolId, Pools, Rank, Team,
};
use campfire_net::JoinState;
use campfire_sim::{EntityIndex, SimTick, Unpredicted};

use crate::hud::gauge::{Cooling, Gauge, GaugeKind};
use crate::hud::ring::Ring;
use crate::view::drawing::Drawing;
use crate::view::look::Look;
use crate::view::{CAMERA, ViewSystems};

mod gauge;
mod ring;

/// Makes the match readable with plain shapes over the drawings: a bar of the life pool over
/// every unit; under the own avatar's, its other pools, a gold mark over each ability it may
/// learn a rank of, its cooldowns, and a tick for each rank of an ability that ranks up, bright
/// once learned; a ring where each hit lands, and a ring under the unit the own avatar attacks.
/// It reads the sim's components and changes none.
#[derive(Debug)]
pub(crate) struct Hud;

/// The mode's life pool, when it has one.
#[derive(Resource, Debug, Clone, Copy)]
struct Life(PoolId);

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
    learnable: Handle<StandardMaterial>,
    hit: Handle<StandardMaterial>,
    target: Handle<StandardMaterial>,
}

/// The scratch `add_gauges` builds each unit's gauges in.
#[derive(Debug, Default)]
struct GaugeScratch {
    kinds: Vec<GaugeKind>,
    ranks: Vec<Ranked>,
}

/// An ability slot whose ability ranks up, and its count of ranks.
#[derive(Debug, Clone, Copy)]
struct Ranked {
    slot: u8,
    ranks: u8,
}

/// The drawn units with no gauges yet: each one's drawing, its team, its pools, and its ability
/// slots.
type Ungauged<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Drawing,
        &'static Team,
        Option<&'static Pools>,
        Option<&'static ActionSlots>,
    ),
    (Without<Gauged>, Allow<Unpredicted>),
>;

/// On a unit: its gauges are made.
#[derive(Component, Debug)]
struct Gauged;

/// What the gauges show of each unit, and its drawing.
type Shown<'w, 's> = Query<
    'w,
    's,
    (
        &'static Drawing,
        Has<Dead>,
        Option<&'static Pools>,
        Option<&'static ActionSlots>,
        Option<&'static Points>,
        Option<&'static Level>,
    ),
    Allow<Unpredicted>,
>;

/// The gauged units whose gauges may show something else this frame: what they show changed, or
/// they are new.
type Restated<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Gauged>,
        Or<(
            Changed<Pools>,
            Changed<ActionSlots>,
            Changed<Points>,
            Changed<Level>,
            Added<Dead>,
            Added<Gauged>,
        )>,
        Allow<Unpredicted>,
    ),
>;

/// The drawn units whose pools changed.
type Hurt<'w, 's> =
    Query<'w, 's, (&'static Pools, &'static Drawing), (Changed<Pools>, Allow<Unpredicted>)>;

/// The fill of each gauge: a drawn quad that no gauge holds.
type Fills<'w, 's> = Query<'w, 's, &'static mut Transform, (With<Mesh3d>, Without<Gauge>)>;

/// The ring under the unit the own avatar attacks, a child of the root of that unit's drawing,
/// which the unit's despawn takes along.
#[derive(Debug, Clone, Copy)]
struct Marked {
    root: Entity,
    ring: Entity,
}

/// How far over a drawing's top its gauges start.
const ABOVE: f32 = 0.35;

impl Plugin for Hud {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (Hud::bind_life, Hud::set_palette));
        app.add_systems(
            Update,
            (
                Hud::add_gauges,
                Hud::mark_hits.run_if(resource_exists::<Life>),
                Hud::fill_gauges,
                Hud::widen_rings,
                Hud::mark_target,
            )
                .chain()
                .after(ViewSystems),
        );
    }
}

impl Hud {
    /// Takes the life pool the sim's combat binds, when the mode names one.
    fn bind_life(world: &mut World) {
        if let Some(life) = Combat::life(world) {
            world.insert_resource(Life(life));
        }
    }

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
        commands.insert_resource(HudPalette {
            quad: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
            ring: meshes.add(Annulus::new(0.55, 0.7).mesh()),
            back: flat(Color::srgb(0.1, 0.1, 0.12)),
            friend: flat(Color::srgb(0.3, 0.85, 0.35)),
            foe: flat(Color::srgb(0.9, 0.3, 0.25)),
            resource: flat(Color::srgb(0.3, 0.55, 1.0)),
            cooldown: flat(Color::srgb(0.9, 0.9, 0.9)),
            learnable: flat(Color::srgb(1.0, 0.72, 0.1)),
            hit: flat(Color::srgb(1.0, 0.8, 0.3)),
            target: flat(Color::srgb(1.0, 0.35, 0.2)),
        });
    }

    /// Gives each drawn unit its gauges, once the client plays, its team telling friend from
    /// foe: life for every unit with the life pool; for the own avatar, each other pool, then,
    /// when an ability ranks up, a row of learn marks, then a cooldown per ability slot, then the
    /// rank ticks of each ability that ranks up. The gauges stand over the drawing's top, facing
    /// the camera, as children of its root.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn add_gauges(
        palette: Res<'_, HudPalette>,
        life: Option<Res<'_, Life>>,
        learning: Option<Learning<'_>>,
        state: Option<Res<'_, JoinState>>,
        players: PlayerUnits<'_, '_>,
        drawn: Ungauged<'_, '_>,
        looks: Query<'_, '_, &Look>,
        mut scratch: Local<'_, GaugeScratch>,
        mut commands: Commands<'_, '_>,
    ) {
        let state = state.as_deref();
        let (Some(slot), Some(own_team)) = (
            state.and_then(JoinState::slot),
            state.and_then(JoinState::team),
        ) else {
            return;
        };
        let avatar = players.avatar(slot).map(|avatar| avatar.entity);
        let life = life.map(|life| life.0);
        let facing = Quat::from_rotation_arc(Vec3::Y, CAMERA.normalize());
        let GaugeScratch { kinds, ranks } = &mut *scratch;
        for (unit, drawing, team, pools, slots) in &drawn {
            commands.entity(unit).insert(Gauged);
            // A projectile's drawing has no look, and no gauges.
            let Ok(look) = looks.get(drawing.root()) else {
                continue;
            };
            let mine = Some(unit) == avatar;
            let friend = *team == own_team;
            kinds.clear();
            let current = |pool| pools.and_then(|pools| pools.current(pool));
            if let Some(life) = life
                && pools.is_some_and(|pools| pools.max(life).is_some())
            {
                kinds.push(GaugeKind::Life {
                    shown: current(life),
                });
            }
            let mut row = 1;
            if mine && let Some(pools) = pools {
                for pool in pools.ids().filter(|&pool| Some(pool) != life) {
                    kinds.push(GaugeKind::Pool { pool, row });
                    row += 1;
                }
            }
            if mine && let Some(slots) = slots {
                ranks.clear();
                ranks.extend((0..).zip(slots.iter()).filter_map(|(slot, held)| {
                    Some(Ranked {
                        slot,
                        ranks: learning.as_ref()?.ranks(held)?,
                    })
                }));
                if !ranks.is_empty() {
                    kinds.extend(ranks.iter().map(|ranked| GaugeKind::Learnable {
                        slot: ranked.slot,
                        row,
                    }));
                    row += 1;
                }
                kinds.extend(
                    (0..)
                        .zip(slots.iter())
                        .map(|(slot, _)| GaugeKind::Cooldown {
                            slot,
                            row,
                            cooling: None,
                        }),
                );
                row += 1;
                for &Ranked { slot, ranks } in &*ranks {
                    kinds.extend((1..=ranks).map(|rank| GaugeKind::Rank {
                        slot,
                        rank,
                        ranks,
                        row,
                    }));
                }
            }
            let top = Vec3::Y * (look.height() + ABOVE);
            for &kind in &*kinds {
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
                        MeshMaterial3d(palette.fill(kind, friend).clone()),
                        layout.fill(0.0),
                    ))
                    .id();
                let gauge = commands
                    .spawn((
                        Gauge { kind, fill },
                        layout.place(top, facing),
                        Visibility::Hidden,
                    ))
                    .add_children(&[back, fill])
                    .id();
                commands.entity(drawing.root()).add_child(gauge);
            }
        }
    }

    /// Puts a ring on the ground where a unit's life dropped since its gauge last showed it.
    fn mark_hits(
        time: Res<'_, Time>,
        palette: Res<'_, HudPalette>,
        life: Res<'_, Life>,
        units: Hurt<'_, '_>,
        roots: Query<'_, '_, (&Transform, &Children)>,
        mut gauges: Query<'_, '_, &mut Gauge>,
        mut commands: Commands<'_, '_>,
    ) {
        for (pools, drawing) in &units {
            let Ok((root, children)) = roots.get(drawing.root()) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some(mut gauge) = gauges.fetch_next() {
                let GaugeKind::Life { shown } = &mut gauge.kind else {
                    continue;
                };
                let current = pools.current(life.0);
                if current
                    .zip(*shown)
                    .is_some_and(|(current, shown)| current < shown)
                {
                    commands.spawn((
                        Mesh3d(palette.ring.clone()),
                        MeshMaterial3d(palette.hit.clone()),
                        Transform::from_translation(root.translation + Vec3::Y * 0.05)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        Ring {
                            since: time.elapsed(),
                        },
                    ));
                }
                *shown = current;
            }
        }
    }

    /// Sets the gauges' fills of each unit whose gauges may show something else, and of the own
    /// avatar, whose cooldowns fill with the ticks; hides the gauges of the dead, of unlearned
    /// abilities' cooldowns, and the learn marks of the abilities the unit may not learn now. A
    /// dead unit may learn, so its learn marks show.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn fill_gauges(
        tick: Option<Res<'_, SimTick>>,
        life: Option<Res<'_, Life>>,
        state: Option<Res<'_, JoinState>>,
        learning: Option<Learning<'_>>,
        players: PlayerUnits<'_, '_>,
        restated: Restated<'_, '_>,
        mut revived: RemovedComponents<'_, '_, Dead>,
        units: Shown<'_, '_>,
        roots: Query<'_, '_, &Children>,
        (mut gauges, mut fills): (Query<'_, '_, (&mut Gauge, &mut Visibility)>, Fills<'_, '_>),
        mut touched: Local<'_, Vec<Entity>>,
    ) {
        let avatar = state
            .as_deref()
            .and_then(JoinState::slot)
            .and_then(|slot| players.avatar(slot))
            .map(|avatar| avatar.entity);
        touched.clear();
        touched.extend(restated.iter().chain(revived.read()).chain(avatar));
        touched.sort_unstable();
        touched.dedup();
        let now = tick.map(|tick| tick.start());
        for &unit in &*touched {
            let Ok((drawing, dead, pools, slots, points, level)) = units.get(unit) else {
                continue;
            };
            let Ok(children) = roots.get(drawing.root()) else {
                continue;
            };
            let mut gauges = gauges.iter_many_mut(children);
            while let Some((mut gauge, mut visibility)) = gauges.fetch_next() {
                let fill = |pool| {
                    let pools = pools?;
                    Some(Gauge::share(pools.current(pool)?, pools.max(pool)?))
                };
                let fraction = match &mut gauge.kind {
                    GaugeKind::Life { .. } => life.as_ref().and_then(|life| fill(life.0)),
                    GaugeKind::Pool { pool, .. } => fill(*pool),
                    GaugeKind::Cooldown { slot, cooling, .. } => {
                        let learned = slots
                            .and_then(|slots| slots.slot(*slot))
                            .filter(|state| state.rank.is_some());
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
                    GaugeKind::Rank { slot, rank, .. } => {
                        slots.and_then(|slots| slots.slot(*slot)).map(|held| {
                            if Rank::count(held.rank) >= *rank {
                                1.0
                            } else {
                                0.0
                            }
                        })
                    }
                    GaugeKind::Learnable { slot, .. } => {
                        let held = slots.and_then(|slots| slots.slot(*slot));
                        let learnable = learning
                            .as_ref()
                            .zip(held)
                            .zip(points.zip(level))
                            .is_some_and(|((learning, held), (&points, &level))| {
                                learning.learnable(held, points, level)
                            });
                        learnable.then_some(1.0)
                    }
                };
                let learns = matches!(gauge.kind, GaugeKind::Learnable { .. });
                let shown = fraction.filter(|_| !dead || learns);
                visibility.set_if_neq(if shown.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if let (Some(fraction), Ok(mut fill)) = (shown, fills.get_mut(gauge.fill)) {
                    fill.set_if_neq(gauge.kind.layout().fill(fraction));
                }
            }
        }
    }

    /// Puts the target ring on the ground under the unit the own avatar attacks, as wide as the
    /// unit's drawing, when the target changes: a child of the target's drawing, which carries it
    /// along. A ring whose unit is gone went with the unit's drawing.
    fn mark_target(
        (index, state, palette): (
            Res<'_, EntityIndex>,
            Option<Res<'_, JoinState>>,
            Res<'_, HudPalette>,
        ),
        players: PlayerUnits<'_, '_>,
        own: Query<'_, '_, &ActionSlots>,
        units: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        looks: Query<'_, '_, &Look>,
        mut marked: Local<'_, Option<Marked>>,
        mut commands: Commands<'_, '_>,
    ) {
        let target = state
            .as_deref()
            .and_then(JoinState::slot)
            .and_then(|slot| players.avatar(slot))
            .and_then(|avatar| own.get(avatar.entity).ok())
            .and_then(ActionSlots::attack_target)
            .and_then(|target| index.get(target))
            .and_then(|target| units.get(target).ok())
            .and_then(|drawing| Some((drawing.root(), looks.get(drawing.root()).ok()?)));
        if marked.map(|marked| marked.root) == target.map(|(root, _)| root) {
            return;
        }
        if let Some(old) = marked.take() {
            commands.entity(old.ring).try_despawn();
        }
        if let Some((root, look)) = target {
            let ring = commands
                .spawn((
                    Mesh3d(palette.ring.clone()),
                    MeshMaterial3d(palette.target.clone()),
                    Transform::from_translation(Vec3::Y * 0.04)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(2.0 * look.radius())),
                    ChildOf(root),
                ))
                .id();
            *marked = Some(Marked { root, ring });
        }
    }

    fn widen_rings(
        time: Res<'_, Time>,
        mut rings: Query<'_, '_, (Entity, &Ring, &mut Transform)>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, ring, mut transform) in &mut rings {
            match ring.scale(time.elapsed()) {
                Some(scale) => transform.scale = Vec3::splat(scale),
                None => commands.entity(entity).despawn(),
            }
        }
    }
}

impl HudPalette {
    /// The material a gauge of `kind` fills with, for a unit of the client's team if `friend`.
    const fn fill(&self, kind: GaugeKind, friend: bool) -> &Handle<StandardMaterial> {
        match kind {
            GaugeKind::Life { .. } if friend => &self.friend,
            GaugeKind::Life { .. } => &self.foe,
            GaugeKind::Pool { .. } => &self.resource,
            GaugeKind::Learnable { .. } => &self.learnable,
            GaugeKind::Cooldown { .. } | GaugeKind::Rank { .. } => &self.cooldown,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position};

    use super::*;
    use crate::view::View;

    #[test]
    fn a_ring_marks_each_drop_of_life_where_the_unit_is_drawn() {
        // Bevy checks that a system's queries do not conflict when it first runs.
        let mut app = App::new();
        app.add_plugins((TimePlugin, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_resource::<EntityIndex>();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        Unpredicted::register(app.world_mut());
        app.add_plugins((View, Hud));
        app.update();
        assert!(app.world().contains_resource::<HudPalette>());
        // With no own avatar there is no target to mark: nothing wears the target's material.
        let target = app.world().resource::<HudPalette>().target.clone();
        let mut worn = app.world_mut().query::<&MeshMaterial3d<StandardMaterial>>();
        assert!(worn.iter(app.world()).all(|material| material.0 != target));

        // A unit of 600 life at (3, 0, −2), whose life gauge showed it full.
        let life = PoolId::new(0).unwrap();
        app.insert_resource(Life(life));
        let pools = |max| Pools::new([(life, Num::int(max))]).unwrap();
        let at = Position::new(campfire_math::Vec3::new(
            Num::int(3),
            Num::ZERO,
            Num::int(-2),
        ));
        let unit = app
            .world_mut()
            .spawn((
                IdAllocator::default().allocate(),
                at.unwrap(),
                Team::new(0),
                pools(600),
            ))
            .id();
        app.update();
        let root = app.world().get::<Drawing>(unit).unwrap().root();
        let shown = |shown| GaugeKind::Life {
            shown: Some(Num::int(shown)),
        };
        let gauge = app
            .world_mut()
            .spawn((
                Gauge {
                    kind: shown(600),
                    fill: Entity::PLACEHOLDER,
                },
                ChildOf(root),
            ))
            .id();
        let mut rings = app.world_mut().query::<(&Ring, &Transform)>();
        // Unchanged, its pools mark nothing; a drop to 450 marks one ring on the ground under it,
        // a rise to 500 none, and the gauge keeps what it showed last.
        app.update();
        assert_eq!(rings.iter(app.world()).count(), 0);
        app.world_mut().entity_mut(unit).insert(pools(450));
        app.update();
        let (_, ring) = rings.single(app.world()).unwrap();
        assert_eq!(ring.translation, Vec3::new(3.0, 0.05, -2.0));
        assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(450));
        app.world_mut().entity_mut(unit).insert(pools(500));
        app.update();
        assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(500));
        assert_eq!(rings.iter(app.world()).count(), 1);
    }
}
