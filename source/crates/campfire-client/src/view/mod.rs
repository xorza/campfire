use std::collections::BTreeMap;

use bevy::app::{App, FixedPostUpdate, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::camera::{Camera3d, ClearColor};
use bevy::color::Color;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::query::{Added, Allow, Changed, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::common_conditions::resource_added;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::DirectionalLight;
use bevy::math::primitives::{Capsule3d, Cuboid, Plane3d, Sphere};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::{Fixed, Time};
use bevy::transform::components::Transform;
use bevy::window::Window;
use bevy::world_serialization::WorldAssetRoot;
use campfire_capabilities::{
    ActionSlots, Area, Body, Dead, Facing, MatchEnd, MatchResult, MoveStep, Owner, Projectile,
    Team, UnitType,
};
use campfire_net::JoinState;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use lightyear::prelude::Predicted;

use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::float_num::FloatNum;
use crate::view::footing::Footing;
use crate::view::glide::{Glide, TickClock};
use crate::view::ground_heights::GroundHeights;
use crate::view::look::{Look, Pose, Shape};
use crate::view::unit_looks::UnitLooks;

pub(crate) mod client_data;
pub(crate) mod drawing;
pub(crate) mod file_material;
pub(crate) mod float_num;
pub(crate) mod footing;
pub(crate) mod glide;
pub(crate) mod ground_heights;
pub(crate) mod look;
pub(crate) mod package_source;
pub(crate) mod unit_looks;
pub(crate) mod unit_models;

/// Draws the match: a camera over the lane, the ground, a capsule for every unit the client holds,
/// colored by team, and a ball for every projectile, each moving smoothly between the places the
/// sim gives it; a unit in its attack's windup leans towards its target. Each unit's drawing is a
/// tree under a root on the ground, which glides, and which the unit's despawn takes along.
#[derive(Debug)]
pub(crate) struct View;

/// The systems that place the drawings, which the HUD and the pointer read after.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ViewSystems;

/// Where the camera stands, looking at the origin.
pub(crate) const CAMERA: Vec3 = Vec3::new(0.0, 24.0, 18.0);

/// The meshes and materials units are drawn with.
#[derive(Resource, Debug)]
struct Palette {
    projectile: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    shot: Handle<StandardMaterial>,
    /// By team index: the first playing team, the second, and any other.
    teams: [Handle<StandardMaterial>; 3],
    dead: Handle<StandardMaterial>,
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
/// walks, whether a player controls it, whether it is the client's own, whether it is dead,
/// whether the client only receives it, its body, its type and the way it faces.
type NewUnits<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static Team,
        Has<MoveStep>,
        Has<Owner>,
        Has<Predicted>,
        Has<Dead>,
        Has<Unpredicted>,
        Option<&'static Body>,
        Option<&'static UnitType>,
        Option<&'static Facing>,
    ),
    (
        With<StableId>,
        Without<Drawing>,
        Without<Projectile>,
        Without<Area>,
        Allow<Unpredicted>,
    ),
>;

/// The drawn units the client only receives whose sim place changed.
type MovedUnits<'w, 's> =
    Query<'w, 's, (&'static Position, &'static Drawing), (Changed<Position>, With<Unpredicted>)>;

/// A projectile in flight: a ball at the height of a unit's chest.
const SHOT_RADIUS: f32 = 0.2;
const SHOT_HEIGHT: f32 = 1.2;

/// How far a unit in its attack's windup leans towards its target, in radians.
const LEAN: f32 = 0.35;

/// The living units with actions, each with the attack it may be in.
type Attackers<'w, 's> =
    Query<'w, 's, (&'static Drawing, &'static ActionSlots), (Without<Dead>, Allow<Unpredicted>)>;

/// The projectiles not drawn yet.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Position, Has<Unpredicted>),
    (With<Projectile>, Without<Drawing>, Allow<Unpredicted>),
>;

impl Plugin for View {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, View::set_scene);
        app.add_systems(FixedPostUpdate, View::step);
        app.add_systems(
            Update,
            (
                View::draw_new,
                View::draw_shots,
                View::mourn,
                View::follow,
                View::glide,
                View::lean,
            )
                .chain()
                .in_set(ViewSystems),
        );
        app.add_systems(Update, View::show_end.run_if(resource_added::<MatchEnd>));
    }
}

impl View {
    fn set_scene(
        mut commands: Commands<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        // No tonemapping: the default one needs lookup tables the client does not build with.
        commands.spawn((
            Camera3d::default(),
            Tonemapping::None,
            Transform::from_translation(CAMERA).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        commands.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.11)));
        commands.spawn((
            DirectionalLight::default(),
            Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        commands.spawn((
            Ground,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(44.0, 20.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.33, 0.28))),
        ));
        let palette = Palette {
            projectile: meshes.add(Sphere::new(SHOT_RADIUS).mesh()),
            own: materials.add(Color::srgb(1.0, 0.85, 0.2)),
            teams: [
                materials.add(Color::srgb(0.25, 0.45, 0.95)),
                materials.add(Color::srgb(0.9, 0.25, 0.2)),
                materials.add(Color::srgb(0.6, 0.6, 0.6)),
            ],
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
        looks: Option<Res<'_, UnitLooks>>,
        data: Option<Res<'_, ClientData>>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut capsules: Local<'_, BTreeMap<[u32; 2], Handle<Mesh>>>,
        mut cuboids: Local<'_, BTreeMap<[u32; 3], Handle<Mesh>>>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.as_deref().and_then(|data| data.heights.as_ref());
        for (unit, &pos, team, walks, owned, own, dead, received, body, unit_type, facing) in &units
        {
            let at = ground(pos, heights);
            let models = looks
                .as_deref()
                .zip(unit_type)
                .map_or(&[][..], |(looks, &unit_type)| looks.models(unit_type));
            if !models.is_empty() {
                let degrees = facing.map_or(0.0, |facing| facing.degrees().float());
                let turn = Transform::from_rotation(Quat::from_rotation_y(degrees.to_radians()));
                let root = commands
                    .spawn((
                        DrawingOf(unit),
                        Transform::from_translation(at),
                        Visibility::default(),
                        View::rest(at, received, &time),
                        Shape::of(owned, walks, body),
                    ))
                    .id();
                for model in models {
                    commands.spawn((
                        WorldAssetRoot(model.scene.clone()),
                        model.parts.clone(),
                        turn,
                        ChildOf(root),
                    ));
                }
                continue;
            }
            let shape = Shape::of(owned, walks, body);
            let mesh = match shape.footing {
                Footing::Circle(_) => capsules
                    .entry([shape.radius.to_bits(), shape.length.to_bits()])
                    .or_insert_with(|| {
                        meshes.add(Capsule3d::new(shape.radius, shape.length).mesh())
                    }),
                Footing::Box { half, .. } => {
                    let size = [2.0 * half[0], shape.height(), 2.0 * half[1]];
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
            } = look.pose(shape, dead, &palette.dead);
            commands.entity(figure).insert((material, transform));
            commands
                .spawn((
                    DrawingOf(unit),
                    Transform::from_translation(at),
                    Visibility::default(),
                    View::rest(at, received, &time),
                    look,
                    shape,
                ))
                .add_child(figure);
        }
    }

    /// Gives each projectile the client received a ball at its place, which glides as a unit's
    /// drawing does.
    fn draw_shots(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        shots: NewShots<'_, '_>,
        data: Option<Res<'_, ClientData>>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.as_deref().and_then(|data| data.heights.as_ref());
        for (shot, &pos, received) in &shots {
            let at = ground(pos, heights);
            let ball = commands
                .spawn((
                    Mesh3d(palette.projectile.clone()),
                    MeshMaterial3d(palette.shot.clone()),
                    Transform::from_translation(Vec3::Y * SHOT_HEIGHT),
                ))
                .id();
            commands
                .spawn((
                    DrawingOf(shot),
                    Transform::from_translation(at),
                    Visibility::default(),
                    View::rest(at, received, &time),
                ))
                .add_child(ball);
        }
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
        for (drawing, slots) in &units {
            let aim = slots
                .attacking()
                .and_then(|target| index.get(target))
                .and_then(|target| drawn.get(target).ok())
                .and_then(|target| roots.get(target.root()).ok())
                .map(|(transform, _, _)| transform.translation);
            // A unit its models draw has no figure to lean.
            let Ok((root, Some(look), shape)) = roots.get(drawing.root()) else {
                continue;
            };
            let Ok(mut figure) = figures.get_mut(look.figure) else {
                continue;
            };
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
            let pose = look.pose(shape, dead, &palette.dead);
            *material = pose.material;
            *transform = pose.transform;
        }
    }

    /// A drawing at rest `at`: one that follows the ticks the client runs, or one that glides to
    /// each place the client receives, if `received`.
    fn rest(at: Vec3, received: bool, time: &Time) -> Glide {
        if received {
            Glide::timed(at, time.elapsed())
        } else {
            Glide::ticked(at)
        }
    }

    /// Gives each predicted unit's drawing the unit's place after the tick, in every tick the
    /// client runs, a rollback's included, so the last two ticks it draws between are the ones
    /// that hold now.
    fn step(
        units: Query<'_, '_, (&Position, &Drawing)>,
        data: Option<Res<'_, ClientData>>,
        mut roots: Query<'_, '_, &mut Glide>,
    ) {
        let heights = data.as_deref().and_then(|data| data.heights.as_ref());
        for (&pos, drawing) in &units {
            if let Ok(mut glide) = roots.get_mut(drawing.root()) {
                glide.tick(ground(pos, heights));
            }
        }
    }

    /// Starts a glide to each received unit's new sim place.
    fn follow(
        time: Res<'_, Time>,
        units: MovedUnits<'_, '_>,
        data: Option<Res<'_, ClientData>>,
        mut roots: Query<'_, '_, (&Transform, &mut Glide)>,
    ) {
        let heights = data.as_deref().and_then(|data| data.heights.as_ref());
        for (&pos, drawing) in &units {
            if let Ok((transform, mut glide)) = roots.get_mut(drawing.root()) {
                glide.head(transform.translation, ground(pos, heights), time.elapsed());
            }
        }
    }

    /// Moves each root along its glide, by the fixed clock as it runs now, at any speed of a
    /// local match.
    fn glide(
        time: Res<'_, Time>,
        fixed: Res<'_, Time<Fixed>>,
        mut roots: Query<'_, '_, (&Glide, &mut Transform)>,
    ) {
        let clock = TickClock {
            timestep: fixed.timestep(),
            fraction: fixed.overstep_fraction(),
        };
        for (glide, mut transform) in &mut roots {
            let translation = glide.at(time.elapsed(), clock);
            // A drawing at rest keeps its transform, unchanged, until it glides again.
            if transform.translation != translation {
                transform.translation = translation;
            }
        }
    }

    /// Shows the match's end for the client's team: the window's title names it, and the ground
    /// turns gold for a victory, dark red for a defeat and gray for a draw.
    fn show_end(
        (end, state): (Res<'_, MatchEnd>, Res<'_, JoinState>),
        ground: Single<'_, '_, &mut MeshMaterial3d<StandardMaterial>, With<Ground>>,
        mut windows: Query<'_, '_, &mut Window>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let standing = Standing::of(end.result(), state.team());
        let (title, color) = match standing {
            Some(Standing::Victory) => ("Campfire: victory", Color::srgb(0.75, 0.6, 0.15)),
            Some(Standing::Defeat) => ("Campfire: defeat", Color::srgb(0.4, 0.08, 0.06)),
            Some(Standing::Draw) | None => {
                ("Campfire: the match ended", Color::srgb(0.35, 0.35, 0.35))
            }
        };
        ground.into_inner().0 = materials.add(color);
        for mut window in &mut windows {
            title.clone_into(&mut window.title);
        }
    }
}

impl Standing {
    /// How `result` stands for the team `own`; `None` for a match someone won when the team is
    /// not known.
    fn of(result: MatchResult, own: Option<Team>) -> Option<Standing> {
        match (result, own) {
            (MatchResult::Draw, _) => Some(Standing::Draw),
            (MatchResult::Won(winner), Some(own)) if winner == own => Some(Standing::Victory),
            (MatchResult::Won(_), Some(_)) => Some(Standing::Defeat),
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

/// A sim place in the renderer's floats: its height above the ground, which `heights` gives
/// beneath it, or 0 on a map with none.
fn ground(pos: Position, heights: Option<&GroundHeights>) -> Vec3 {
    let at = pos.get();
    let (x, z) = (at.x.float(), at.z.float());
    let beneath = heights.map_or(0.0, |heights| heights.at(x, z));
    Vec3::new(x, beneath + at.y.float(), z)
}

#[cfg(test)]
mod tests;
