use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::{Camera3d, ClearColor};
use bevy::color::Color;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Despawn;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_added;
use bevy::ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy::light::DirectionalLight;
use bevy::math::primitives::{Capsule3d, Plane3d};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy::window::Window;
use campfire_capabilities::{Dead, MatchEnd, MatchResult, MoveStep, Owner, Team};
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

/// Where the camera stands, looking at the origin.
pub(crate) const CAMERA: Vec3 = Vec3::new(0.0, 24.0, 18.0);

/// The meshes and materials units are drawn with.
#[derive(Resource, Debug)]
struct Palette {
    avatar: Handle<Mesh>,
    creep: Handle<Mesh>,
    structure: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    /// By team index: the first playing team, the second, and any other.
    teams: [Handle<StandardMaterial>; 3],
    dead: Handle<StandardMaterial>,
}

#[derive(Resource, Debug)]
struct TickSeconds(f32);

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

/// On a sim unit: the entity that draws it. The sim entity may be `Unpredicted`, which the
/// renderer does not see, so the drawing is an entity of its own.
#[derive(Component, Debug)]
pub(crate) struct Drawn(Entity);

/// Where a drawing moves: from where it was drawn when the unit's sim place last changed, to
/// that place, over one tick from `since`, in seconds of app time; `lift` raises the capsule to
/// stand on the ground.
#[derive(Component, Debug)]
pub(crate) struct Glide {
    from: Vec3,
    to: Vec3,
    since: f32,
    lift: f32,
}

/// The units not drawn yet: where each stands, its team, whether it walks, whether a player
/// controls it, whether it is the client's own, and whether it is dead.
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
    ),
    (With<StableId>, Without<Drawn>, Allow<Unpredicted>),
>;

/// The drawn units whose sim place changed.
type MovedUnits<'w, 's> =
    Query<'w, 's, (&'static Position, &'static Drawn), (Changed<Position>, Allow<Unpredicted>)>;

/// How a drawing looks while its unit lives: its material and its shape. A dead unit lies on the
/// ground, gray.
#[derive(Component, Debug)]
pub(crate) struct Look {
    alive: Handle<StandardMaterial>,
    shape: Shape,
}

/// The shape of a unit: an avatar is under a player's control, a structure does not walk.
#[derive(Debug, Clone, Copy)]
struct Shape {
    radius: f32,
    length: f32,
}

const AVATAR: Shape = Shape {
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
        app.add_systems(
            Update,
            (View::draw_new, View::mourn, View::follow, View::glide).chain(),
        );
        app.add_systems(Update, View::show_end.run_if(resource_added::<MatchEnd>));
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
        let mut capsule =
            |shape: Shape| meshes.add(Capsule3d::new(shape.radius, shape.length).mesh());
        let palette = Palette {
            avatar: capsule(AVATAR),
            creep: capsule(CREEP),
            structure: capsule(STRUCTURE),
            own: materials.add(Color::srgb(1.0, 0.85, 0.2)),
            teams: [
                materials.add(Color::srgb(0.25, 0.45, 0.95)),
                materials.add(Color::srgb(0.9, 0.25, 0.2)),
                materials.add(Color::srgb(0.6, 0.6, 0.6)),
            ],
            dead: materials.add(Color::srgb(0.22, 0.22, 0.24)),
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
        for (unit, &pos, team, walks, owned, own, dead) in &units {
            let (shape, mesh) = match (owned, walks) {
                (true, _) => (AVATAR, &palette.avatar),
                (false, true) => (CREEP, &palette.creep),
                (false, false) => (STRUCTURE, &palette.structure),
            };
            let material = if own {
                &palette.own
            } else {
                &palette.teams[usize::from(team.index()).min(2)]
            };
            let look = Look {
                alive: material.clone(),
                shape,
            };
            let at = ground(pos);
            let mut glide = Glide {
                from: at,
                to: at,
                since: time.elapsed_secs(),
                lift: 0.0,
            };
            let mut transform = Transform::default();
            let mut material = MeshMaterial3d(material.clone());
            look.show(dead, &palette, &mut material, &mut transform, &mut glide);
            transform.translation = at + Vec3::Y * glide.lift;
            let drawing = commands
                .spawn((Mesh3d(mesh.clone()), material, transform, glide, look))
                .id();
            commands.entity(unit).insert(Drawn(drawing));
        }
    }

    /// Lays each unit that died down, and stands each unit that came back to life up.
    fn mourn(
        palette: Res<'_, Palette>,
        died: Query<'_, '_, &Drawn, (Added<Dead>, Allow<Unpredicted>)>,
        mut revived: RemovedComponents<'_, '_, Dead>,
        units: Query<'_, '_, &Drawn, Allow<Unpredicted>>,
        mut drawings: Query<
            '_,
            '_,
            (
                &Look,
                &mut MeshMaterial3d<StandardMaterial>,
                &mut Transform,
                &mut Glide,
            ),
        >,
    ) {
        let revived = revived.read().filter_map(|unit| units.get(unit).ok());
        let changes = died.iter().map(|drawn| (drawn, true));
        for (&Drawn(drawing), dead) in changes.chain(revived.map(|drawn| (drawn, false))) {
            if let Ok((look, mut material, mut transform, mut glide)) = drawings.get_mut(drawing) {
                look.show(dead, &palette, &mut material, &mut transform, &mut glide);
            }
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
            glide.from = glide.ground(transform);
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

    /// Shows the match's end for the client's team: the window's title names it, and the ground
    /// turns gold for a victory, dark red for a defeat and gray for a draw.
    fn show_end(
        end: Res<'_, MatchEnd>,
        own: Query<'_, '_, &Team, With<Predicted>>,
        ground: Single<'_, '_, &mut MeshMaterial3d<StandardMaterial>, With<Ground>>,
        mut windows: Query<'_, '_, &mut Window>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        let standing = Standing::of(end.result(), own.iter().next().copied());
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

impl Glide {
    /// Where on the ground a drawing at `transform` stands.
    pub(crate) fn ground(&self, transform: &Transform) -> Vec3 {
        transform.translation - Vec3::Y * self.lift
    }
}

impl Drawn {
    pub(crate) const fn drawing(&self) -> Entity {
        self.0
    }
}

impl Look {
    /// How far the drawing reaches from its axis.
    pub(crate) const fn radius(&self) -> f32 {
        self.shape.radius
    }

    /// How tall the drawing stands while its unit lives.
    pub(crate) fn height(&self) -> f32 {
        self.shape.length + 2.0 * self.shape.radius
    }

    /// Sets a drawing's material, pose and lift for a unit that is `dead` or alive.
    fn show(
        &self,
        dead: bool,
        palette: &Palette,
        material: &mut MeshMaterial3d<StandardMaterial>,
        transform: &mut Transform,
        glide: &mut Glide,
    ) {
        let Shape { radius, length } = self.shape;
        if dead {
            material.0 = palette.dead.clone();
            transform.rotation = Quat::from_rotation_z(FRAC_PI_2);
            glide.lift = radius;
        } else {
            material.0 = self.alive.clone();
            transform.rotation = Quat::IDENTITY;
            glide.lift = radius + length / 2.0;
        }
    }
}

/// A sim place on the ground plane, in the renderer's floats.
fn ground(pos: Position) -> Vec3 {
    let at = pos.get();
    Vec3::new(float(at.x), 0.0, float(at.z))
}

/// A sim number in the renderer's floats.
#[expect(
    clippy::cast_precision_loss,
    reason = "drawing needs no more than an f32's 24 bits of a place"
)]
pub(crate) fn float(value: Num) -> f32 {
    value.to_bits() as f32 / (1_u64 << Num::FRAC_BITS) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_result_stands_by_the_clients_team() {
        let [a, b] = [0, 1].map(Team::new);
        let won = MatchResult::Won(a);
        assert_eq!(Standing::of(won, Some(a)), Some(Standing::Victory));
        assert_eq!(Standing::of(won, Some(b)), Some(Standing::Defeat));
        assert_eq!(Standing::of(won, None), None);
        for own in [Some(a), None] {
            assert_eq!(Standing::of(MatchResult::Draw, own), Some(Standing::Draw));
        }
    }
}
