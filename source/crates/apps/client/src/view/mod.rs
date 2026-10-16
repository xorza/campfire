use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;

use bevy::app::{App, FixedPostUpdate, Plugin, Startup, Update};
use bevy::asset::{AssetServer, Assets, Handle};
use bevy::camera::ClearColor;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::DirectionalLight;
use bevy::material::AlphaMode;
use bevy::math::primitives::{Annulus, Capsule3d, Cuboid, Cylinder, Plane3d, Sphere};
use bevy::math::{Affine3A, EulerRot, Quat, Vec3, Vec3A};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::{Fixed, Time};
use bevy::transform::components::Transform;
use bevy::window::Window;
use bevy::world_serialization::WorldAssetRoot;
use campfire_actions::ActionProgress;
use campfire_areas::{Area, AreaReach};
use campfire_mode::{MatchEnd, MatchResult};
use campfire_net::JoinState;
use campfire_projectiles::Projectile;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Avatar, Body, BodyUnit, Dead, Facing, MoveStep};
use campfire_values::{NameId, Team, UnitType};
use lightyear::prelude::Predicted;

use crate::selection::{Selection, SelectionSystems};
use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::float_num::FloatNum;
use crate::view::footing::Footing;
use crate::view::glide::{Follows, Glide, Spot, TickClock};
use crate::view::ground_heights::GroundHeights;
use crate::view::look::{Condition, Look, Pose, Shape, ShapeKind};
use crate::view::scene_lights::{SceneLights, Sun};
use crate::view::unit_looks::{ModelParts, UnitLooks};
use crate::view::unit_models::Dressed;

pub(crate) mod camera_rig;
pub(crate) mod client_data;
pub(crate) mod drawing;
pub(crate) mod file_material;
pub(crate) mod float_num;
pub(crate) mod footing;
pub(crate) mod glide;
pub(crate) mod ground_heights;
pub(crate) mod look;
pub(crate) mod package_source;
pub(crate) mod player_camera;
pub(crate) mod scene_lights;
pub(crate) mod terrain_image;
pub(crate) mod terrain_meshes;
pub(crate) mod terrain_view;
pub(crate) mod unit_looks;
pub(crate) mod unit_models;

/// Draws the match: a camera over the lane, the ground, a capsule for every unit the client holds,
/// colored by team, a projectile's models turned along its flight or a ball, and a disc for every
/// area, each moving smoothly between the places the sim gives it; a unit in its attack's windup
/// leans towards its target. Each unit's drawing is a tree under a root on the ground, which
/// glides, and which the unit's despawn takes along.
#[derive(Debug)]
pub(crate) struct View;

/// The systems that place the drawings, which the HUD and the pointer read after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ViewSystems;

/// The meshes and materials units are drawn with.
#[derive(Resource, Debug)]
struct Palette {
    projectile: Handle<Mesh>,
    /// A disc of radius 1, which an area's drawing scales to its radius.
    area: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    shot: Handle<StandardMaterial>,
    /// A see-through tint, which shows the ground under an area.
    reach: Handle<StandardMaterial>,
    /// By team index: the first playing team, the second, and any other.
    teams: [Handle<StandardMaterial>; 3],
    dead: Handle<StandardMaterial>,
    /// A flat ring whose inner radius is 1, which a selected unit's ring scales to its reach.
    ring: Handle<Mesh>,
    selected: Handle<StandardMaterial>,
}

/// The ring under a selected unit, a child of the root of the unit's drawing, which the unit's
/// despawn takes along.
#[derive(Debug, Clone, Copy)]
struct Ringed {
    unit: StableId,
    ring: Entity,
}

/// On the root of a model drawing whose shape's height waits for its models' bounds, which their
/// meshes get once their scenes spawn.
#[derive(Component, Debug)]
struct FitsModel;

/// What the walk of a model drawing's tree reads of each node: its place in its parent, its
/// children, the bounds of its mesh, whether its look hides it, and whether it holds a mesh.
type ModelNodes<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        Option<&'static Children>,
        Option<&'static Aabb>,
        Option<&'static Visibility>,
        Has<Mesh3d>,
    ),
>;

/// What the fit of a model drawing reads of each model under its root: whether its scene is
/// dressed, and its scene, whose load may fail.
type DrawnModels<'w, 's> = Query<'w, 's, (Has<Dressed>, &'static WorldAssetRoot), With<ModelParts>>;

/// What the fit of a model drawing finds: a model or a mesh still to come, the top of what it
/// shows, or nothing shown, as of a model whose look hides it all or whose file does not load.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fit {
    Waits,
    Top(f32),
    Nothing,
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

/// The areas not drawn yet: where each lies, its type, and whether the client only receives it.
type NewAreas<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static UnitType,
        Has<Unpredicted>,
    ),
    (With<Area>, Without<Drawing>, Allow<Unpredicted>),
>;

/// An area's disc: how thick it is, and how far over the ground it lies, so the ground does not
/// hide it.
const AREA_THICKNESS: f32 = 0.02;
const AREA_LIFT: f32 = 0.03;

/// A selected unit's ring: its width, as a share of its inner radius, and how far over the
/// ground it lies, over an area's disc and under the HUD's target ring.
const RING_WIDTH: f32 = 0.12;
const RING_LIFT: f32 = 0.035;

/// The match's end, in the frame it reaches the match's entity, which the client holds
/// unpredicted.
type Ended<'w, 's> = Query<'w, 's, &'static MatchEnd, (Added<MatchEnd>, Allow<Unpredicted>)>;

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

impl Plugin for View {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::set_scene);
        app.add_systems(FixedPostUpdate, Self::step);
        app.add_systems(
            Update,
            (
                Self::draw_new,
                Self::fit_models,
                Self::draw_shots,
                Self::draw_areas,
                Self::mourn,
                Self::follow,
                Self::head_shots,
                Self::glide,
                Self::lean,
            )
                .chain()
                .in_set(ViewSystems),
        );
        app.add_systems(Update, Self::show_end);
        app.add_systems(
            Update,
            Self::ring_selection
                .after(ViewSystems)
                .after(SelectionSystems),
        );
    }
}

impl View {
    /// Sets the background, the map's lights or one plain sun, the view's own ground unless the
    /// map has a terrain, and the palette.
    fn set_scene(
        data: Res<'_, ClientData>,
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
            commands.spawn((
                Ground,
                Mesh3d(meshes.add(Plane3d::default().mesh().size(44.0, 20.0))),
                MeshMaterial3d(materials.add(Color::srgb(0.3, 0.33, 0.28))),
            ));
        }
        let palette = Palette {
            projectile: meshes.add(Sphere::new(SHOT_RADIUS).mesh()),
            area: meshes.add(Cylinder::new(1.0, AREA_THICKNESS).mesh()),
            own: materials.add(Color::srgb(1.0, 0.85, 0.2)),
            teams: [
                materials.add(Color::srgb(0.25, 0.45, 0.95)),
                materials.add(Color::srgb(0.9, 0.25, 0.2)),
                materials.add(Color::srgb(0.6, 0.6, 0.6)),
            ],
            dead: materials.add(Color::srgb(0.22, 0.22, 0.24)),
            shot: materials.add(Color::srgb(1.0, 0.95, 0.6)),
            reach: materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.2, 0.35),
                alpha_mode: AlphaMode::Blend,
                ..StandardMaterial::default()
            }),
            ring: meshes.add(Annulus::new(1.0, 1.0 + RING_WIDTH).mesh()),
            selected: materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.95, 0.4),
                unlit: true,
                ..StandardMaterial::default()
            }),
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
        looks: Option<Res<'_, UnitLooks>>,
        data: Res<'_, ClientData>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut capsules: Local<'_, BTreeMap<[u32; 2], Handle<Mesh>>>,
        mut cuboids: Local<'_, BTreeMap<[u32; 3], Handle<Mesh>>>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.heights.as_ref();
        for (unit, &pos, team, walks, avatar, own, dead, received, body, &unit_type, facing) in
            &units
        {
            let at = ground(pos, heights);
            match looks.as_deref().and_then(|looks| looks.look(unit_type)) {
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
                    let root = commands
                        .spawn((
                            DrawingOf(unit),
                            Turns,
                            Transform::from_translation(at)
                                .with_rotation(Quat::from_rotation_y(spot.yaw)),
                            Visibility::default(),
                            Glide::rest(spot, Follows::of(received), time.elapsed()),
                            Shape::of(ShapeKind::of(avatar, walks), body),
                            FitsModel,
                        ))
                        .id();
                    for model in models {
                        commands.spawn((
                            WorldAssetRoot(model.scene.clone()),
                            model.parts.clone(),
                            Transform::from_scale(Vec3::splat(model.scale)),
                            ChildOf(root),
                        ));
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
                &palette.teams[team.index().min(2)]
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
        looks: Option<Res<'_, UnitLooks>>,
        data: Res<'_, ClientData>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.heights.as_ref();
        for (shot, &pos, &unit_type, received) in &shots {
            let at = ground(pos, heights);
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
            match looks.as_deref().and_then(|looks| looks.look(unit_type)) {
                Some(models) => {
                    commands.entity(root).insert(Heading { from: at });
                    for model in models {
                        commands.spawn((
                            WorldAssetRoot(model.scene.clone()),
                            model.parts.clone(),
                            Transform::from_scale(Vec3::splat(model.scale)),
                            ChildOf(root),
                        ));
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
        let heights = data.heights.as_ref();
        for (&pos, drawing) in &shots {
            let Ok((mut heading, mut transform)) = roots.get_mut(drawing.root()) else {
                continue;
            };
            let to = ground(pos, heights);
            if let Some(turn) = Heading::along(to - heading.from) {
                transform.rotation = turn;
            }
            heading.from = to;
        }
    }

    /// Gives each area the client received a drawing where it lies: a see-through disc of its
    /// radius on the ground, which glides as a unit's drawing does.
    fn draw_areas(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        areas: NewAreas<'_, '_>,
        reach: Option<AreaReach<'_>>,
        data: Res<'_, ClientData>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.heights.as_ref();
        for (area, &pos, &unit_type, received) in &areas {
            let radius = reach
                .as_ref()
                .and_then(|reach| reach.radius(unit_type))
                .expect("an area's type has its reach")
                .float();
            let disc = commands
                .spawn((
                    Mesh3d(palette.area.clone()),
                    MeshMaterial3d(palette.reach.clone()),
                    Transform::from_translation(Vec3::Y * AREA_LIFT)
                        .with_scale(Vec3::new(radius, 1.0, radius)),
                ))
                .id();
            let at = ground(pos, heights);
            commands
                .spawn((
                    DrawingOf(area),
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
                .add_child(disc);
        }
    }

    /// Gives each model drawing's shape the height of its models' top over its root, once each
    /// of its models is dressed or failed to load, and each mesh it shows has its bounds: a model
    /// is as tall as it is drawn, whatever its unit's kind. A drawing that shows nothing keeps its
    /// shape's height, and its fit ends all the same, so no drawing is walked again.
    fn fit_models(
        mut roots: Query<'_, '_, (Entity, &mut Shape), With<FitsModel>>,
        (models, nodes): (DrawnModels<'_, '_>, ModelNodes<'_, '_>),
        assets: Res<'_, AssetServer>,
        mut walk: Local<'_, Vec<(Entity, Affine3A)>>,
        mut commands: Commands<'_, '_>,
    ) {
        for (root, mut shape) in &mut roots {
            match Self::fit(root, (&models, &nodes), &assets, &mut walk) {
                Fit::Waits => continue,
                Fit::Top(top) => shape.height = top.max(0.0),
                Fit::Nothing => {}
            }
            commands.entity(root).remove::<FitsModel>();
        }
    }

    /// The fit of the drawing at `root`, its models' trees walked through `walk`: a node its look
    /// hides counts for nothing, nor do the nodes under it.
    fn fit(
        root: Entity,
        (models, nodes): (&DrawnModels<'_, '_>, &ModelNodes<'_, '_>),
        assets: &AssetServer,
        walk: &mut Vec<(Entity, Affine3A)>,
    ) -> Fit {
        walk.clear();
        if let Ok((_, Some(children), ..)) = nodes.get(root) {
            for &child in children {
                let Ok((dressed, scene)) = models.get(child) else {
                    continue;
                };
                if dressed {
                    walk.push((child, Affine3A::IDENTITY));
                } else if !assets.load_state(&scene.0).is_failed() {
                    return Fit::Waits;
                }
            }
        }
        let mut top = None::<f32>;
        while let Some((node, parent)) = walk.pop() {
            let Ok((transform, children, bounds, visibility, mesh)) = nodes.get(node) else {
                continue;
            };
            if visibility == Some(&Visibility::Hidden) {
                continue;
            }
            let at = parent * transform.compute_affine();
            match bounds {
                Some(bounds) => {
                    let reach = Self::bounds_top(at, bounds);
                    top = Some(top.map_or(reach, |top| top.max(reach)));
                }
                None if mesh => return Fit::Waits,
                None => {}
            }
            if let Some(children) = children {
                walk.extend(children.iter().map(|&child| (child, at)));
            }
        }
        top.map_or(Fit::Nothing, Fit::Top)
    }

    /// The highest point of `bounds` placed by `at`, exactly: the height of its center, and each
    /// half extent times how far its axis rises.
    fn bounds_top(at: Affine3A, bounds: &Aabb) -> f32 {
        let rise = Vec3A::new(
            at.matrix3.x_axis.y,
            at.matrix3.y_axis.y,
            at.matrix3.z_axis.y,
        );
        at.translation.y + rise.dot(bounds.center) + rise.abs().dot(bounds.half_extents)
    }

    /// Leans each living unit in its attack's windup towards its target, and stands every other
    /// living unit with actions upright; a figure's pose changes only when its lean does.
    fn lean(
        index: Res<'_, EntityIndex>,
        units: Attackers<'_, '_>,
        drawn: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        roots: Query<'_, '_, (&Transform, Option<&Look>, &Shape), Without<Figure>>,
        mut figures: Query<'_, '_, &mut Transform, With<Figure>>,
    ) {
        for (drawing, progress) in &units {
            // A unit its models draw has no figure to lean, nor a target to look up.
            let Ok((root, Some(look), shape)) = roots.get(drawing.root()) else {
                continue;
            };
            let Ok(mut figure) = figures.get_mut(look.figure) else {
                continue;
            };
            let aim = progress
                .attacking()
                .and_then(|target| index.get(target))
                .and_then(|target| drawn.get(target).ok())
                .and_then(|target| roots.get(target.root()).ok())
                .map(|(transform, _, _)| transform.translation);
            let lean = aim.map_or(Quat::IDENTITY, |to| lean_toward(root.translation, to));
            let rotation = lean * shape.footing.upright();
            if figure.rotation != rotation {
                figure.rotation = rotation;
            }
        }
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
        let heights = data.heights.as_ref();
        for (&pos, facing, drawing) in &units {
            if let Ok(mut glide) = roots.get_mut(drawing.root()) {
                glide.tick(Spot {
                    point: ground(pos, heights),
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
        let heights = data.heights.as_ref();
        for (&pos, facing, drawing) in &units {
            if let Ok((transform, mut glide)) = roots.get_mut(drawing.root()) {
                let drawn = Spot {
                    point: transform.translation,
                    yaw: transform.rotation.to_euler(EulerRot::YXZ).0,
                };
                let to = Spot {
                    point: ground(pos, heights),
                    yaw: yaw(facing),
                };
                glide.head(drawn, to, time.elapsed());
            }
        }
    }

    /// Rings each selected unit on the ground, at its reach, once its drawing stands, and takes
    /// the ring from a unit that leaves the selection. It looks again when the selection
    /// changes, or while a selected unit waits for its drawing.
    fn ring_selection(
        selection: Res<'_, Selection>,
        index: Res<'_, EntityIndex>,
        palette: Res<'_, Palette>,
        units: Query<'_, '_, &Drawing, Allow<Unpredicted>>,
        shapes: Query<'_, '_, &Shape>,
        mut ringed: Local<'_, Vec<Ringed>>,
        mut commands: Commands<'_, '_>,
    ) {
        let selected = selection.get();
        if !selection.is_changed() && ringed.len() == selected.len() {
            return;
        }
        ringed.retain(|ringed| {
            let kept = selected.binary_search(&ringed.unit).is_ok();
            if !kept {
                commands.entity(ringed.ring).try_despawn();
            }
            kept
        });
        let before = ringed.len();
        for &unit in selected {
            if ringed[..before]
                .binary_search_by_key(&unit, |ringed| ringed.unit)
                .is_ok()
            {
                continue;
            }
            let Some(root) = index
                .get(unit)
                .and_then(|unit| units.get(unit).ok())
                .map(Drawing::root)
            else {
                continue;
            };
            let Ok(shape) = shapes.get(root) else {
                continue;
            };
            let ring = commands
                .spawn((
                    Mesh3d(palette.ring.clone()),
                    MeshMaterial3d(palette.selected.clone()),
                    Transform::from_translation(Vec3::Y * RING_LIFT)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(shape.reach())),
                    ChildOf(root),
                ))
                .id();
            ringed.push(Ringed { unit, ring });
        }
        ringed.sort_unstable_by_key(|ringed| ringed.unit);
    }

    /// Moves each root along its glide, by the fixed clock as it runs now, at any speed of a
    /// local match.
    fn glide(
        time: Res<'_, Time>,
        fixed: Res<'_, Time<Fixed>>,
        mut roots: Query<'_, '_, (&Glide, &mut Transform, Has<Turns>)>,
    ) {
        let clock = TickClock {
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

    /// Shows the match's end for the client's team, as it reaches the match's entity: the
    /// window's title names it, and the view's own ground turns gold for a victory, dark red for
    /// a defeat and gray for a draw.
    fn show_end(
        (ended, state): (Ended<'_, '_>, Option<Res<'_, JoinState>>),
        ground: Option<Single<'_, '_, &mut MeshMaterial3d<StandardMaterial>, With<Ground>>>,
        mut windows: Query<'_, '_, &mut Window>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let Some(end) = ended.iter().next() else {
            return;
        };
        let state = state.expect("a client that holds the match's end joined it");
        let standing = Standing::of(end.result(), state.team());
        let (title, color) = match standing {
            Some(Standing::Victory) => ("Campfire: victory", Color::srgb(0.75, 0.6, 0.15)),
            Some(Standing::Defeat) => ("Campfire: defeat", Color::srgb(0.4, 0.08, 0.06)),
            Some(Standing::Draw) | None => {
                ("Campfire: the match ended", Color::srgb(0.35, 0.35, 0.35))
            }
        };
        if let Some(ground) = ground {
            ground.into_inner().0 = materials.add(color);
        }
        for mut window in &mut windows {
            title.clone_into(&mut window.title);
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
/// beneath it, or 0 on a map with none.
fn ground(pos: Position, heights: Option<&GroundHeights>) -> Vec3 {
    let point = pos.get();
    let (x, z) = (point.x.float(), point.z.float());
    let beneath = heights.map_or(0.0, |heights| heights.at(x, z));
    Vec3::new(x, beneath + point.y.float(), z)
}

#[cfg(test)]
mod tests;
