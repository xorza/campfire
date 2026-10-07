use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::{Camera3d, ClearColor};
use bevy::color::Color;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::{Despawn, RemovedComponents};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Changed, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_added;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::light::DirectionalLight;
use bevy::math::primitives::{Capsule3d, Cuboid, Plane3d, Sphere};
use bevy::math::{Quat, Vec2, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy::window::Window;
use campfire_capabilities::{
    ActionSlots, Area, Body, Dead, MatchEnd, MatchResult, MoveStep, Owner, Projectile, Team,
};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use lightyear::prelude::Predicted;

/// Draws the match: a camera over the lane, the ground, a capsule for every unit the client holds,
/// colored by team, and a ball for every projectile, each moving smoothly between the places the
/// sim gives it; a unit in its attack's windup leans towards its target.
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
    projectile: Handle<Mesh>,
    own: Handle<StandardMaterial>,
    shot: Handle<StandardMaterial>,
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

/// The units not drawn yet, projectiles and areas apart: where each stands, its team, whether it
/// walks, whether a player controls it, whether it is the client's own, whether it is dead, and its
/// body.
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
        Option<&'static Body>,
    ),
    (
        With<StableId>,
        Without<Drawn>,
        Without<Projectile>,
        Without<Area>,
        Allow<Unpredicted>,
    ),
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

/// The shape of a unit: an avatar is under a player's control, a structure does not walk. A
/// capsule of `radius` and `length`, or, for a box body, a cuboid as tall standing on its
/// `footing`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shape {
    radius: f32,
    length: f32,
    footing: Footing,
}

/// What a unit's drawing covers of the ground round its center: a circle of a radius, or a box
/// of half sides `half` along its turned axes, turned `yaw` radians about the vertical, as Bevy
/// turns a transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Footing {
    Circle(f32),
    Box { half: [f32; 2], yaw: f32 },
}

impl Shape {
    /// The shape of a unit that is under a player's control if `owned`, walks if `walks`, and has
    /// `body`: its kind's height, at its body's radius, or on its box, or its kind's own with no
    /// body.
    fn of(owned: bool, walks: bool, body: Option<&Body>) -> Shape {
        let kind = match (owned, walks) {
            (true, _) => AVATAR,
            (false, true) => CREEP,
            (false, false) => STRUCTURE,
        };
        let footing = match (
            body.and_then(|body| body.radius()),
            body.and_then(|body| body.half_edges()),
        ) {
            (Some(radius), _) => Footing::Circle(View::float(radius)),
            (None, Some([a, b])) => {
                let [a, b] =
                    [a, b].map(|edge| Vec2::new(View::float(edge[0]), View::float(edge[1])));
                Footing::Box {
                    half: [a.length(), b.length()],
                    yaw: (-a.y).atan2(a.x),
                }
            }
            (None, None) => Footing::Circle(kind.radius),
        };
        let radius = match footing {
            Footing::Circle(radius) => radius,
            Footing::Box { .. } => kind.radius,
        };
        Shape {
            radius,
            footing,
            ..kind
        }
    }

    /// How tall it stands.
    fn height(&self) -> f32 {
        self.length + 2.0 * self.radius
    }
}

impl Footing {
    /// Whether it covers the point `offset` from its center, edge included.
    pub(crate) fn covers(self, offset: Vec2) -> bool {
        match self {
            Footing::Circle(radius) => offset.length_squared() <= radius * radius,
            Footing::Box { half, yaw } => {
                // The point in the box's own axes: Bevy turns x by `yaw` to (cos, −sin).
                let along = Vec2::new(yaw.cos(), -yaw.sin());
                let across = Vec2::new(yaw.sin(), yaw.cos());
                offset.dot(along).abs() <= half[0] && offset.dot(across).abs() <= half[1]
            }
        }
    }

    /// The radius of the least circle round its center that holds it.
    fn bound(self) -> f32 {
        match self {
            Footing::Circle(radius) => radius,
            Footing::Box { half, .. } => Vec2::from(half).length(),
        }
    }

    /// The turn it stands at: a box's yaw, none for a circle.
    fn upright(self) -> Quat {
        match self {
            Footing::Circle(_) => Quat::IDENTITY,
            Footing::Box { yaw, .. } => Quat::from_rotation_y(yaw),
        }
    }
}

const AVATAR: Shape = Shape {
    radius: 0.5,
    length: 1.0,
    footing: Footing::Circle(0.5),
};
const CREEP: Shape = Shape {
    radius: 0.35,
    length: 0.5,
    footing: Footing::Circle(0.35),
};
const STRUCTURE: Shape = Shape {
    radius: 0.9,
    length: 2.0,
    footing: Footing::Circle(0.9),
};

/// A projectile in flight: a ball at the height of a unit's chest.
const SHOT_RADIUS: f32 = 0.2;
const SHOT_HEIGHT: f32 = 1.2;

/// How far a unit in its attack's windup leans towards its target, in radians.
const LEAN: f32 = 0.35;

/// The drawn units, with their attack if they have one, and whether they are dead.
type Attackers<'w, 's> =
    Query<'w, 's, (&'static Drawn, Option<&'static ActionSlots>, Has<Dead>), Allow<Unpredicted>>;

/// The projectiles not drawn yet.
type NewShots<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Position),
    (With<Projectile>, Without<Drawn>, Allow<Unpredicted>),
>;

impl Plugin for View {
    fn build(&self, app: &mut App) {
        app.insert_resource(TickSeconds(self.tick.as_secs_f32()));
        app.add_systems(Startup, View::set_scene);
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
                .chain(),
        );
        app.add_systems(Update, View::show_end.run_if(resource_added::<MatchEnd>));
        app.add_observer(View::erase);
    }
}

impl View {
    /// A sim number in the renderer's floats.
    #[expect(
        clippy::cast_precision_loss,
        reason = "drawing needs no more than an f32's 24 bits of a place"
    )]
    pub(crate) const fn float(value: Num) -> f32 {
        value.to_bits() as f32 / (1_u64 << Num::FRAC_BITS) as f32
    }

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

    /// Gives each unit the client received a drawing at its place.
    fn draw_new(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        units: NewUnits<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut capsules: Local<'_, BTreeMap<[u32; 2], Handle<Mesh>>>,
        mut cuboids: Local<'_, BTreeMap<[u32; 3], Handle<Mesh>>>,
        mut commands: Commands<'_, '_>,
    ) {
        for (unit, &pos, team, walks, owned, own, dead, body) in &units {
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
            let mut glide = Glide::resting(at, time.elapsed_secs(), 0.0);
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

    /// Gives each projectile the client received a ball at its place, which glides as a unit's
    /// drawing does.
    fn draw_shots(
        palette: Res<'_, Palette>,
        time: Res<'_, Time>,
        shots: NewShots<'_, '_>,
        mut commands: Commands<'_, '_>,
    ) {
        for (shot, &pos) in &shots {
            let at = ground(pos);
            let glide = Glide::resting(at, time.elapsed_secs(), SHOT_HEIGHT);
            let drawing = commands
                .spawn((
                    Mesh3d(palette.projectile.clone()),
                    MeshMaterial3d(palette.shot.clone()),
                    Transform::from_translation(at + Vec3::Y * SHOT_HEIGHT),
                    glide,
                ))
                .id();
            commands.entity(shot).insert(Drawn(drawing));
        }
    }

    /// Leans each living unit in its attack's windup towards its target, and stands every other
    /// living unit upright.
    fn lean(
        index: Res<'_, EntityIndex>,
        units: Attackers<'_, '_>,
        mut drawings: Query<'_, '_, (&Glide, &Look, &mut Transform)>,
    ) {
        for (&Drawn(drawing), slots, dead) in &units {
            if dead {
                continue;
            }
            let aim = slots
                .and_then(ActionSlots::attacking)
                .and_then(|target| index.get(target))
                .and_then(|target| units.get(target).ok())
                .and_then(|(&Drawn(target), ..)| drawings.get(target).ok())
                .map(|(glide, _, transform)| glide.ground(transform));
            let Ok((glide, look, mut transform)) = drawings.get_mut(drawing) else {
                continue;
            };
            let from = glide.ground(&transform);
            let lean = aim.map_or(Quat::IDENTITY, |to| lean_toward(from, to));
            let rotation = lean * look.shape.footing.upright();
            if transform.rotation != rotation {
                transform.rotation = rotation;
            }
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
            let translation = glide.from.lerp(glide.to, done) + Vec3::Y * glide.lift;
            // A drawing at rest keeps its transform, unchanged, until it glides again.
            if transform.translation != translation {
                transform.translation = translation;
            }
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
    /// A drawing at rest `at`, from `since` seconds of app time, raised by `lift`.
    const fn resting(at: Vec3, since: f32, lift: f32) -> Glide {
        Glide {
            from: at,
            to: at,
            since,
            lift,
        }
    }

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
    /// How far the drawing reaches from its axis: the radius of the least circle that holds what
    /// it covers of the ground.
    pub(crate) fn radius(&self) -> f32 {
        self.shape.footing.bound()
    }

    /// What the drawing covers of the ground.
    pub(crate) const fn footing(&self) -> Footing {
        self.shape.footing
    }

    /// How tall the drawing stands while its unit lives.
    pub(crate) fn height(&self) -> f32 {
        self.shape.height()
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
        let Shape {
            radius,
            length,
            footing,
        } = self.shape;
        material.0 = if dead {
            palette.dead.clone()
        } else {
            self.alive.clone()
        };
        // A capsule lies down when its unit dies; a box stands where it stood, as a ruin.
        if dead && matches!(footing, Footing::Circle(_)) {
            transform.rotation = Quat::from_rotation_z(FRAC_PI_2);
            glide.lift = radius;
        } else {
            transform.rotation = footing.upright();
            glide.lift = radius + length / 2.0;
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

/// A sim place on the ground plane, in the renderer's floats.
fn ground(pos: Position) -> Vec3 {
    let at = pos.get();
    Vec3::new(View::float(at.x), 0.0, View::float(at.z))
}

#[cfg(test)]
mod tests {
    use campfire_capabilities::BodyForm;

    use super::*;

    #[test]
    fn a_unit_leans_its_top_towards_its_target() {
        // Towards +x: the top, (0, 1, 0), turns by 0.35 rad to (sin 0.35, cos 0.35, 0).
        let from = Vec3::new(1.0, 0.0, 2.0);
        let top = lean_toward(from, Vec3::new(5.0, 0.0, 2.0)) * Vec3::Y;
        let expected = Vec3::new(LEAN.sin(), LEAN.cos(), 0.0);
        // A rotation in f32 rounds each component to some ulps of 1, below 1e-6.
        assert!(top.abs_diff_eq(expected, 1e-6), "{top}");
        // Towards −z, whatever the target's height: the top to (0, cos 0.35, −sin 0.35).
        let top = lean_toward(from, Vec3::new(1.0, 3.0, -4.0)) * Vec3::Y;
        let expected = Vec3::new(0.0, LEAN.cos(), -LEAN.sin());
        assert!(top.abs_diff_eq(expected, 1e-6), "{top}");
        // On one spot, no lean.
        assert_eq!(lean_toward(from, from), Quat::IDENTITY);
    }

    #[test]
    fn a_unit_is_drawn_at_its_bodys_radius_and_its_kinds_height() {
        let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
        let avatar = Shape {
            radius: 0.75,
            footing: Footing::Circle(0.75),
            ..AVATAR
        };
        assert_eq!(Shape::of(true, true, Some(&body)), avatar);
        assert_eq!(Shape::of(false, true, None), CREEP);
        assert_eq!(Shape::of(false, false, None), STRUCTURE);
        // A box of 4 × 2 m turned a quarter: its first half edge points along +z, which Bevy's
        // turn of -π/2 about y takes x to; it stands as tall as a structure, and reaches √5 m.
        let form = BodyForm::boxed([Num::int(4), Num::int(2)]).unwrap();
        let boxed = Shape::of(false, false, Some(&form.at(Num::int(90))));
        let expected = Footing::Box {
            half: [2.0, 1.0],
            yaw: -FRAC_PI_2,
        };
        assert_eq!(boxed.footing, expected);
        assert_eq!(boxed.height(), STRUCTURE.height());
        assert!(
            (Look {
                alive: Handle::default(),
                shape: boxed
            }
            .radius()
                - 5.0_f32.sqrt())
            .abs()
                < 1e-6
        );
        assert!(
            Quat::from_rotation_y(-FRAC_PI_2)
                .mul_vec3(Vec3::X)
                .abs_diff_eq(Vec3::Z, 1e-6)
        );
        // It covers a point 1.9 m along z and 0.9 m along x from its center, and not 1.1 m along x.
        assert!(expected.covers(Vec2::new(0.9, 1.9)));
        assert!(!expected.covers(Vec2::new(1.1, 0.0)));
    }

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
