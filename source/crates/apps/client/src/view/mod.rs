use std::collections::BTreeMap;

use bevy::app::{App, FixedPostUpdate, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::ClearColor;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::lifecycle::{Add, RemovedComponents};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::DirectionalLight;
use bevy::math::{EulerRot, Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::shape::{Capsule3d, Cuboid, Plane3d, Sphere};
use bevy::time::{Fixed, Time};
use bevy::transform::components::Transform;
use bevy::window::Window;
use campfire_actions::ActionProgress;
use campfire_mode::{MatchEnd, MatchResult};
use campfire_net::JoinState;
use campfire_net_replication::Predicted;
use campfire_projectiles::Projectile;
use campfire_sim::{EntityIndex, Position, Unpredicted};
use campfire_units::{Avatar, Body, BodyUnit, Dead, Facing, MapBook, MoveStep};
use campfire_values::{Team, UnitType};

use crate::engine_text::{EngineMessage, EngineText};
use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::float_num::FloatNum;
use crate::view::footing::Footing;
use crate::view::glide::{Follows, FrameClock, Glide, Spot};
use crate::view::ground_heights::GroundHeights;
use crate::view::look::{Condition, Look, Pose, Shape, ShapeKind};
use crate::view::match_colors::{MatchColors, Paint};
use crate::view::scene_lights::{SceneLights, Sun};
use crate::view::unit_looks::UnitLooks;

pub(crate) mod area_view;
pub(crate) mod camera_rig;
pub(crate) mod client_data;
pub(crate) mod drawing;
pub(crate) mod file_material;
pub(crate) mod float_num;
pub(crate) mod footing;
pub(crate) mod glide;
pub(crate) mod ground_heights;
pub(crate) mod look;
pub(crate) mod match_colors;
pub(crate) mod package_source;
pub(crate) mod player_camera;
pub(crate) mod scene_lights;
pub(crate) mod terrain_image;
pub(crate) mod terrain_meshes;
pub(crate) mod terrain_view;
pub(crate) mod unit_looks;
pub(crate) mod unit_models;

/// Draws the match: the map's lights, its ground as a plane over its bounds where no terrain draws
/// it, a capsule for every unit the client holds, colored by team, a projectile's models turned
/// along its flight or a ball, and a disc for every area, each moving smoothly between the places
/// the sim gives it; a unit in its attack's windup leans towards its target. Each unit's drawing is
/// a tree under a root on the ground, which glides, and which the unit's despawn takes along.
#[derive(Debug)]
pub(crate) struct ViewPlugin;

/// The systems that place the drawings, which the HUD and the pointer read after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ViewSystems;

/// The systems of `ViewSystems` that give the new units, projectiles and areas their drawings,
/// before the drawings move.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DrawSystems;

/// The meshes and materials units are drawn with.
#[derive(Resource, Debug)]
struct Palette {
    projectile: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    shot: Handle<StandardMaterial>,
    /// By the place of a team's color in the mode's team colors.
    teams: Vec<Handle<StandardMaterial>>,
    dead: Handle<StandardMaterial>,
}

/// On a drawing's root that turns as its unit faces: one of models, which turn whole; a figure
/// leans by its own rotation, and turns none.
#[derive(Component, Debug)]
struct Turns;

/// On the root of a projectile's model drawing, which turns along its flight: the point its sim
/// place was drawn at last, where its next step starts (02-engine-core.md, The client
/// draws a projectile with its look).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct Heading {
    from: Vec3,
}

/// The ground the match is drawn on, which shows the match's result once it ends.
#[derive(Component, Debug)]
struct Ground;

/// How the match ended for the client's team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    Victory,
    Defeat,
    Draw,
}

/// On the figure of a drawing: the child of its root that wears the mesh, and leans and lies down.
#[derive(Component, Debug)]
struct Figure;

/// The units not drawn yet, projectiles and areas apart: where each stands, its team, whether it
/// walks, whether it is a player's avatar, whether it is the client's own, whether it is dead,
/// whether the client only receives it, its body, its type and the way it faces.
type NewUnits<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static Team,
        Has<MoveStep>,
        Has<Avatar>,
        Has<Predicted>,
        Has<Dead>,
        Has<Unpredicted>,
        Option<&'static Body>,
        &'static UnitType,
        Option<&'static Facing>,
    ),
    (With<BodyUnit>, Without<Drawing>, Allow<Unpredicted>),
>;

/// The drawn units the client only receives whose sim place or facing changed.
type MovedUnits<'w, 's> = Query<
    'w,
    's,
    (&'static Position, Option<&'static Facing>, &'static Drawing),
    (Or<(Changed<Position>, Changed<Facing>)>, With<Unpredicted>),
>;

/// A projectile in flight: a ball at the height of a unit's chest.
const SHOT_RADIUS: f32 = 0.2;
const SHOT_HEIGHT: f32 = 1.2;

/// How far a unit in its attack's windup leans towards its target, in radians.
const LEAN: f32 = 0.35;

/// The living units with actions, each with the attack it may be in.
type Attackers<'w, 's> =
    Query<'w, 's, (&'static Drawing, &'static ActionProgress), (Without<Dead>, Allow<Unpredicted>)>;

/// The living units whose actions changed, each with the attack it may now be in.
type Started<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ActionProgress),
    (Changed<ActionProgress>, Without<Dead>, Allow<Unpredicted>),
>;

/// The projectiles not drawn yet: where each is, its type, and whether the client only receives
/// it.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static UnitType,
        Has<Unpredicted>,
    ),
    (With<Projectile>, Without<Drawing>, Allow<Unpredicted>),
>;

/// The drawn projectiles whose sim place changed.
type MovedShots<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static Drawing),
    (With<Projectile>, Changed<Position>, Allow<Unpredicted>),
>;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EngineText::english());
        app.add_systems(Startup, Self::set_scene);
        app.add_systems(FixedPostUpdate, Self::step);
        app.configure_sets(Update, DrawSystems.in_set(ViewSystems));
        app.add_systems(
            Update,
            (Self::draw_new, Self::draw_shots)
                .chain()
                .in_set(DrawSystems),
        );
        app.add_systems(
            Update,
            (
                Self::mourn,
                Self::follow,
                Self::head_shots,
                Self::glide,
                Self::lean,
            )
                .chain()
                .after(DrawSystems)
                .in_set(ViewSystems),
        );
        app.add_observer(Self::show_end);
    }
}

impl ViewPlugin {
    /// Sets the background, the map's lights or one plain sun, the view's own ground over the
    /// map's bounds unless the map has a terrain, and the palette.
    fn set_scene(
        (data, map): (Res<'_, ClientData>, Res<'_, MapBook>),
        mut commands: Commands<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        commands.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.11)));
        match &data.lights {
            Some(lights) => {
                let SceneLights { ambient, suns } = SceneLights::of(lights);
                commands.insert_resource(ambient);
                for Sun { light, transform } in suns {
                    commands.spawn((light, transform));
                }
            }
            None => {
                commands.spawn((
                    DirectionalLight::default(),
                    Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
                ));
            }
        }
        if data.terrain.is_none() {
            let [min, max] = [map.bounds.min(), map.bounds.max()].map(|at| at.map(FloatNum::float));
            let center = Vec3::new(
                f32::midpoint(min[0], max[0]),
                0.0,
                f32::midpoint(min[1], max[1]),
            );
            let plane = Plane3d::default()
                .mesh()
                .size(max[0] - min[0], max[1] - min[1]);
            commands.spawn((
                Ground,
                Mesh3d(meshes.add(plane)),
                MeshMaterial3d(materials.add(Color::srgb(0.3, 0.33, 0.28))),
                Transform::from_translation(center),
            ));
        }
        let palette = Palette {
            projectile: meshes.add(Sphere::new(SHOT_RADIUS).mesh()),
            own: materials.add(Color::srgb(1.0, 0.85, 0.2)),
            teams: MatchColors::new(data.team_colors.clone())
                .teams()
                .map(|color| materials.add(color))
                .collect(),
            dead: materials.add(Color::srgb(0.22, 0.22, 0.24)),
            shot: materials.add(Color::srgb(1.0, 0.95, 0.6)),
        };
        commands.insert_resource(palette);
    }

    /// Gives each unit the client received a drawing at its place: a root at its height above the
    /// ground, and over it its type's models, turned the way it faces, or the figure of its
    /// shape, posed as the unit lives or lies dead.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn draw_new(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        units: NewUnits<'_, '_>,
        looks: Res<'_, UnitLooks>,
        data: Res<'_, ClientData>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut capsules: Local<'_, BTreeMap<[u32; 2], Handle<Mesh>>>,
        mut cuboids: Local<'_, BTreeMap<[u32; 3], Handle<Mesh>>>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.lifts();
        for (unit, &position, team, walks, avatar, own, dead, received, body, &unit_type, facing) in
            &units
        {
            let at = ground(position, heights);
            match looks.look(unit_type) {
                // A look of no models draws nothing: its drawing has no shape to point at, ring
                // or gauge, only the place its unit stands.
                Some([]) => {
                    let spot = Spot {
                        point: at,
                        yaw: 0.0,
                    };
                    commands.spawn((
                        DrawingOf(unit),
                        Transform::from_translation(at),
                        Visibility::default(),
                        Glide::rest(spot, Follows::of(received), time.elapsed()),
                    ));
                    continue;
                }
                Some(models) => {
                    let spot = Spot {
                        point: at,
                        yaw: yaw(facing),
                    };
                    // A model is as tall as it is drawn, whatever its unit's kind.
                    let mut shape = Shape::of(ShapeKind::of(avatar, walks), body);
                    if let Some(top) = looks.top(unit_type) {
                        shape.height = top.max(0.0);
                    }
                    let root = commands
                        .spawn((
                            DrawingOf(unit),
                            Turns,
                            Transform::from_translation(at)
                                .with_rotation(Quat::from_rotation_y(spot.yaw)),
                            Visibility::default(),
                            Glide::rest(spot, Follows::of(received), time.elapsed()),
                            shape,
                        ))
                        .id();
                    for model in models {
                        commands.spawn((model.bundle(Paint::of(*team)), ChildOf(root)));
                    }
                    continue;
                }
                None => {}
            }
            let shape = Shape::of(ShapeKind::of(avatar, walks), body);
            let mesh = match shape.footing {
                Footing::Circle(_) => capsules
                    .entry([shape.radius.to_bits(), shape.length().to_bits()])
                    .or_insert_with(|| {
                        meshes.add(Capsule3d::new(shape.radius, shape.length()).mesh())
                    }),
                Footing::Box { half, .. } => {
                    let size = [2.0 * half[0], shape.height, 2.0 * half[1]];
                    cuboids.entry(size.map(f32::to_bits)).or_insert_with(|| {
                        meshes.add(Cuboid::new(size[0], size[1], size[2]).mesh())
                    })
                }
            };
            let alive = if own {
                &palette.own
            } else {
                &palette.teams[data.team_colors.place(*team)]
            };
            let figure = commands.spawn((Figure, Mesh3d(mesh.clone()))).id();
            let look = Look {
                figure,
                alive: alive.clone(),
            };
            let Pose {
                material,
                transform,
            } = look.pose(shape, Condition::of(dead), &palette.dead);
            commands.entity(figure).insert((material, transform));
            commands
                .spawn((
                    DrawingOf(unit),
                    Transform::from_translation(at),
                    Visibility::default(),
                    Glide::rest(
                        Spot {
                            point: at,
                            yaw: 0.0,
                        },
                        Follows::of(received),
                        time.elapsed(),
                    ),
                    look,
                    shape,
                ))
                .add_child(figure);
        }
    }

    /// Gives each projectile the client received a drawing at its place, its height included,
    /// which glides as a unit's drawing does: its type's models, which turn along its flight, or
    /// nothing for a look of no models, or with no look a ball at a chest's height above it.
    fn draw_shots(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        shots: NewShots<'_, '_>,
        looks: Res<'_, UnitLooks>,
        data: Res<'_, ClientData>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.lifts();
        for (shot, &position, &unit_type, received) in &shots {
            let at = ground(position, heights);
            let root = commands
                .spawn((
                    DrawingOf(shot),
                    Transform::from_translation(at),
                    Visibility::default(),
                    Glide::rest(
                        Spot {
                            point: at,
                            yaw: 0.0,
                        },
                        Follows::of(received),
                        time.elapsed(),
                    ),
                ))
                .id();
            match looks.look(unit_type) {
                Some(models) => {
                    commands.entity(root).insert(Heading { from: at });
                    for model in models {
                        commands.spawn((model.bundle(Paint::default()), ChildOf(root)));
                    }
                }
                None => {
                    commands.spawn((
                        Mesh3d(palette.projectile.clone()),
                        MeshMaterial3d(palette.shot.clone()),
                        Transform::from_translation(Vec3::Y * SHOT_HEIGHT),
                        ChildOf(root),
                    ));
                }
            }
        }
    }

    /// Turns each projectile's model drawing along its last step, the way from the point its sim
    /// place was drawn at before to the one now, its model's `+x` along it and its top up; a
    /// step of no length keeps its turn.
    fn head_shots(
        shots: MovedShots<'_, '_>,
        data: Res<'_, ClientData>,
        mut roots: Query<'_, '_, (&mut Heading, &mut Transform)>,
    ) {
        let heights = data.lifts();
        for (&position, drawing) in &shots {
            let Ok((mut heading, mut transform)) = roots.get_mut(drawing.root()) else {
                continue;
            };
            let to = ground(position, heights);
            if let Some(turn) = Heading::along(to - heading.from) {
                transform.rotation = turn;
            }
            heading.from = to;
        }
    }

    /// Leans each living unit in its attack's windup towards its target, kept as its attack
    /// starts, and stands it upright again as the attack ends; a figure's pose changes only when
    /// its lean does. A unit that dies leaves the leaning ones, its pose then its death's.
    fn lean(
        index: Res<'_, EntityIndex>,
        (started, attackers): (Started<'_, '_>, Attackers<'_, '_>),
        drawn: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        roots: Query<'_, '_, (&Transform, Option<&Look>, &Shape), Without<Figure>>,
        mut figures: Query<'_, '_, &mut Transform, With<Figure>>,
        mut leaning: Local<'_, Vec<Entity>>,
    ) {
        for (unit, progress) in &started {
            if progress.attacking().is_some() && !leaning.contains(&unit) {
                leaning.push(unit);
            }
        }
        leaning.retain(|&unit| {
            let Ok((drawing, progress)) = attackers.get(unit) else {
                return false;
            };
            let attacking = progress.attacking();
            // A unit its models draw has no figure to lean, nor a target to look up.
            let Ok((root, Some(look), shape)) = roots.get(drawing.root()) else {
                return attacking.is_some();
            };
            let Ok(mut figure) = figures.get_mut(look.figure) else {
                return attacking.is_some();
            };
            let aim = attacking
                .and_then(|target| index.get(target))
                .and_then(|target| drawn.get(target).ok())
                .and_then(|target| roots.get(target.root()).ok())
                .map(|(transform, _, _)| transform.translation);
            let lean = aim.map_or(Quat::IDENTITY, |to| lean_toward(root.translation, to));
            let rotation = lean * shape.footing.upright();
            if figure.rotation != rotation {
                figure.rotation = rotation;
            }
            attacking.is_some()
        });
    }

    /// Poses each unit whose death or return to life the client learned this frame as it stands
    /// now, dead or alive: a rollback across a predicted death both takes `Dead` away and gives
    /// it back in one frame, and the unit's present state is what it shows.
    fn mourn(
        palette: Res<'_, Palette>,
        died: Query<'_, '_, Entity, (Added<Dead>, Allow<Unpredicted>)>,
        mut revived: RemovedComponents<'_, '_, Dead>,
        units: Query<'_, '_, (&Drawing, Has<Dead>), Allow<Unpredicted>>,
        roots: Query<'_, '_, (&Look, &Shape)>,
        mut figures: Query<
            '_,
            '_,
            (&mut MeshMaterial3d<StandardMaterial>, &mut Transform),
            With<Figure>,
        >,
        mut touched: Local<'_, Vec<Entity>>,
    ) {
        touched.clear();
        touched.extend(died.iter().chain(revived.read()));
        touched.sort_unstable();
        touched.dedup();
        for &unit in &*touched {
            let Ok((drawing, dead)) = units.get(unit) else {
                continue;
            };
            let Ok((look, &shape)) = roots.get(drawing.root()) else {
                continue;
            };
            let Ok((mut material, mut transform)) = figures.get_mut(look.figure) else {
                continue;
            };
            let pose = look.pose(shape, Condition::of(dead), &palette.dead);
            *material = pose.material;
            *transform = pose.transform;
        }
    }

    /// Gives each predicted unit's drawing the unit's place and facing after the tick, in every
    /// tick the client runs, a rollback's included, so the last two ticks it draws between are
    /// the ones that hold now.
    fn step(
        units: Query<'_, '_, (&Position, Option<&Facing>, &Drawing)>,
        data: Res<'_, ClientData>,
        mut roots: Query<'_, '_, &mut Glide>,
    ) {
        let heights = data.lifts();
        for (&position, facing, drawing) in &units {
            if let Ok(mut glide) = roots.get_mut(drawing.root()) {
                glide.tick(Spot {
                    point: ground(position, heights),
                    yaw: yaw(facing),
                });
            }
        }
    }

    /// Starts a glide to each received unit's new sim place and facing.
    fn follow(
        time: Res<'_, Time>,
        units: MovedUnits<'_, '_>,
        data: Res<'_, ClientData>,
        mut roots: Query<'_, '_, (&Transform, &mut Glide)>,
    ) {
        let heights = data.lifts();
        for (&position, facing, drawing) in &units {
            if let Ok((transform, mut glide)) = roots.get_mut(drawing.root()) {
                let drawn = Spot {
                    point: transform.translation,
                    yaw: transform.rotation.to_euler(EulerRot::YXZ).0,
                };
                let to = Spot {
                    point: ground(position, heights),
                    yaw: yaw(facing),
                };
                glide.head(drawn, to, time.elapsed());
            }
        }
    }

    /// Moves each root along its glide, by the fixed clock as it runs now, at any speed of a
    /// local match.
    fn glide(
        time: Res<'_, Time>,
        fixed: Res<'_, Time<Fixed>>,
        mut roots: Query<'_, '_, (&Glide, &mut Transform, Has<Turns>)>,
    ) {
        let clock = FrameClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        for (glide, mut transform, turns) in &mut roots {
            let spot = glide.at(time.elapsed(), clock);
            // A drawing at rest keeps its transform, unchanged, until it glides again.
            if transform.translation != spot.point {
                transform.translation = spot.point;
            }
            let rotation = Quat::from_rotation_y(spot.yaw);
            if turns && transform.rotation != rotation {
                transform.rotation = rotation;
            }
        }
    }

    /// Shows the match's end for the client's team, as it reaches the match's entity: the window's
    /// title names it in the engine's text, and the view's own ground turns gold for a victory,
    /// dark red for a defeat and gray for a draw.
    fn show_end(
        added: On<'_, '_, Add<MatchEnd>>,
        (ends, state, text): (
            Query<'_, '_, &MatchEnd, Allow<Unpredicted>>,
            Res<'_, JoinState>,
            Res<'_, EngineText>,
        ),
        ground: Option<Single<'_, '_, &mut MeshMaterial3d<StandardMaterial>, With<Ground>>>,
        mut windows: Query<'_, '_, &mut Window>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let end = ends.get(added.entity).expect("the match's end it reached");
        let standing = Standing::of(end.result(), state.team());
        let (title, color) = match standing {
            Some(Standing::Victory) => (EngineMessage::Victory, Color::srgb(0.75, 0.6, 0.15)),
            Some(Standing::Defeat) => (EngineMessage::Defeat, Color::srgb(0.4, 0.08, 0.06)),
            Some(Standing::Draw) | None => {
                (EngineMessage::MatchEnded, Color::srgb(0.35, 0.35, 0.35))
            }
        };
        let title = text.text(title);
        if let Some(ground) = ground {
            ground.into_inner().0 = materials.add(color);
        }
        for mut window in &mut windows {
            window.title.clone_from(&title);
        }
    }
}

impl Heading {
    /// The turn that points `+x` along `way` with `+y` up: its yaw about the vertical, then its
    /// pitch above the ground plane; none for a way of no length.
    fn along(way: Vec3) -> Option<Quat> {
        if way == Vec3::ZERO {
            return None;
        }
        let level = way.x.hypot(way.z);
        let yaw = (-way.z).atan2(way.x);
        let pitch = way.y.atan2(level);
        Some(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch))
    }
}

impl Standing {
    /// How `result` stands for the team `own`; `None` for a match someone won when the team is
    /// not known.
    fn of(result: MatchResult, own: Option<Team>) -> Option<Self> {
        match (result, own) {
            (MatchResult::Draw, _) => Some(Self::Draw),
            (MatchResult::Won(winner), Some(own)) if winner == own => Some(Self::Victory),
            (MatchResult::Won(_), Some(_)) => Some(Self::Defeat),
            (MatchResult::Won(_), None) => None,
        }
    }
}

/// The turn that leans a standing drawing at `from` by `LEAN` towards `to` on the ground plane:
/// about the horizontal axis square to the way to `to`, so its top moves towards `to`. No lean
/// when the two stand on one spot.
fn lean_toward(from: Vec3, to: Vec3) -> Quat {
    let way = Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero();
    Quat::from_axis_angle(
        Vec3::Y.cross(way).normalize_or(Vec3::X),
        LEAN * way.length(),
    )
}

/// The yaw a drawing turns by, in radians, of a unit facing `facing`; none for one that faces
/// none.
fn yaw(facing: Option<&Facing>) -> f32 {
    facing.map_or(0.0, |facing| facing.degrees().float().to_radians())
}

/// A sim place in the renderer's floats: its height above the ground, which `heights` gives
/// beneath it, or above 0 on a map with none or a spatial map's, as `ClientData::lifts` gives.
fn ground(position: Position, heights: Option<&GroundHeights>) -> Vec3 {
    let point = position.get();
    let (x, z) = (point.x.float(), point.z.float());
    let beneath = heights.map_or(0.0, |heights| heights.at(x, z));
    Vec3::new(x, beneath + point.y.float(), z)
}

#[cfg(test)]
mod tests;
