use std::time::Duration;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::{Camera3d, ClearColor};
use bevy::color::Color;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Despawn;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, Changed, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::light::DirectionalLight;
use bevy::math::Vec3;
use bevy::math::primitives::{Capsule3d, Plane3d};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use campfire_capabilities::{MoveStep, Owner, Team};
use campfire_math::Num;
use campfire_net::Unpredicted;
use campfire_sim::{Position, StableId};
use lightyear::prelude::Predicted;

/// Draws the match: a camera over the lane, the ground, and a capsule for every unit the client
/// holds, colored by team, moving smoothly between the places the sim gives it.
#[derive(Debug)]
pub(crate) struct View {
    /// How long a tick lasts: a drawn unit takes one tick to reach its sim place.
    pub(crate) tick: Duration,
}

/// The meshes and materials units are drawn with.
#[derive(Resource, Debug)]
struct Palette {
    hero: Handle<Mesh>,
    creep: Handle<Mesh>,
    structure: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    /// By team index: the first playing team, the second, and any other.
    teams: [Handle<StandardMaterial>; 3],
}

#[derive(Resource, Debug)]
struct TickSeconds(f32);

/// On a sim unit: the entity that draws it. The sim entity may be `Unpredicted`, which the
/// renderer does not see, so the drawing is an entity of its own.
#[derive(Component, Debug)]
struct Drawn(Entity);

/// Where a drawing moves: from where it was drawn when the unit's sim place last changed, to
/// that place, over one tick from `since`, in seconds of app time; `lift` raises the capsule to
/// stand on the ground.
#[derive(Component, Debug)]
struct Glide {
    from: Vec3,
    to: Vec3,
    since: f32,
    lift: f32,
}

/// The units not drawn yet: where each stands, its team, whether it walks, whether a player
/// controls it, and whether it is the client's own.
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
    ),
    (With<StableId>, Without<Drawn>, Allow<Unpredicted>),
>;

/// The drawn units whose sim place changed.
type MovedUnits<'w, 's> =
    Query<'w, 's, (&'static Position, &'static Drawn), (Changed<Position>, Allow<Unpredicted>)>;

/// The shape of a unit: a hero is under a player's control, a structure does not walk.
#[derive(Debug, Clone, Copy)]
struct Shape {
    radius: f32,
    length: f32,
}

const HERO: Shape = Shape {
    radius: 0.5,
    length: 1.0,
};
const CREEP: Shape = Shape {
    radius: 0.35,
    length: 0.5,
};
const STRUCTURE: Shape = Shape {
    radius: 0.9,
    length: 2.0,
};

impl Plugin for View {
    fn build(&self, app: &mut App) {
        app.insert_resource(TickSeconds(self.tick.as_secs_f32()));
        app.add_systems(Startup, View::set_scene);
        app.add_systems(Update, (View::draw_new, View::follow, View::glide).chain());
        app.add_observer(View::erase);
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
            Transform::from_xyz(0.0, 24.0, 18.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        commands.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.11)));
        commands.spawn((
            DirectionalLight::default(),
            Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(44.0, 20.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.33, 0.28))),
        ));
        let mut capsule =
            |shape: Shape| meshes.add(Capsule3d::new(shape.radius, shape.length).mesh());
        let palette = Palette {
            hero: capsule(HERO),
            creep: capsule(CREEP),
            structure: capsule(STRUCTURE),
            own: materials.add(Color::srgb(1.0, 0.85, 0.2)),
            teams: [
                materials.add(Color::srgb(0.25, 0.45, 0.95)),
                materials.add(Color::srgb(0.9, 0.25, 0.2)),
                materials.add(Color::srgb(0.6, 0.6, 0.6)),
            ],
        };
        commands.insert_resource(palette);
    }

    /// Gives each unit the client received a drawing at its place.
    fn draw_new(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        units: NewUnits<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        for (unit, &pos, team, walks, owned, own) in &units {
            let (shape, mesh) = match (owned, walks) {
                (true, _) => (HERO, &palette.hero),
                (false, true) => (CREEP, &palette.creep),
                (false, false) => (STRUCTURE, &palette.structure),
            };
            let material = if own {
                &palette.own
            } else {
                &palette.teams[usize::from(team.index()).min(2)]
            };
            let lift = shape.radius + shape.length / 2.0;
            let at = ground(pos);
            let drawing = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(at + Vec3::Y * lift),
                    Glide {
                        from: at,
                        to: at,
                        since: time.elapsed_secs(),
                        lift,
                    },
                ))
                .id();
            commands.entity(unit).insert(Drawn(drawing));
        }
    }

    /// Starts a glide to each unit's new sim place.
    fn follow(
        time: Res<'_, Time>,
        units: MovedUnits<'_, '_>,
        mut drawings: Query<'_, '_, (&Transform, &mut Glide)>,
    ) {
        for (&pos, &Drawn(drawing)) in &units {
            let Ok((transform, mut glide)) = drawings.get_mut(drawing) else {
                continue;
            };
            glide.from = transform.translation - Vec3::Y * glide.lift;
            glide.to = ground(pos);
            glide.since = time.elapsed_secs();
        }
    }

    fn glide(
        time: Res<'_, Time>,
        tick: Res<'_, TickSeconds>,
        mut drawings: Query<'_, '_, (&Glide, &mut Transform)>,
    ) {
        for (glide, mut transform) in &mut drawings {
            let done = ((time.elapsed_secs() - glide.since) / tick.0).clamp(0.0, 1.0);
            transform.translation = glide.from.lerp(glide.to, done) + Vec3::Y * glide.lift;
        }
    }

    /// Erases a unit's drawing when the unit leaves the client's world.
    fn erase(
        despawned: On<'_, '_, Despawn, Drawn>,
        units: Query<'_, '_, &Drawn, Allow<Unpredicted>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(&Drawn(drawing)) = units.get(despawned.entity) {
            commands.entity(drawing).despawn();
        }
    }
}

/// A sim place on the ground plane, in the renderer's floats.
fn ground(pos: Position) -> Vec3 {
    let at = pos.get();
    Vec3::new(meters(at.x), 0.0, meters(at.z))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "drawing needs no more than an f32's 24 bits of a place"
)]
fn meters(value: Num) -> f32 {
    value.to_bits() as f32 / (1_u64 << Num::FRAC_BITS) as f32
}
