use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::{Allow, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::material::AlphaMode;
use bevy::math::primitives::Cuboid;
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use campfire_capabilities::Order;
use campfire_math::{Num, Vec3 as SimVec3};
use campfire_net::PendingOrders;
use campfire_orders::OrdersAction;
use campfire_production::{BuildBook, Placement};
use campfire_sim::{EntityIndex, Position, Unpredicted};
use campfire_units::{BuildTarget, UnitGrid};
use campfire_values::Team;
use campfire_vision::ByHolding;

use crate::orders::OrderSystems;
use crate::placing::Placing;
use crate::pointer::Pointer;
use crate::selection::SelectionSystems;
use crate::view::float_num::{self, FloatNum};
use crate::view::look::Shape;
use crate::view::unit_looks::UnitLooks;

/// The build preview (02-engine-core.md, A command panel from the actions): while the player places
/// a build, its building's look follows the ground point under the cursor, at its type's angle,
/// over its box drawn green where it may stand and red where it overlaps a wall or a body the
/// client holds; a left click on green orders the build there, and a right click or Esc drops it.
/// It reads the clicks before the selection and the orders, which leave them to it while it places.
#[derive(Debug)]
pub(crate) struct BuildPreview;

/// The root of the preview's drawing: the build it shows, its box's figure, the point it stands
/// at, none while the cursor is off the ground, and whether the building may stand there.
#[derive(Component, Debug)]
struct Ghost {
    placing: Placing,
    footprint: Entity,
    at: Option<SimVec3>,
    fit: Fit,
}

/// Whether a building may stand where the preview shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    Clear,
    Blocked,
}

/// The see-through tints of the preview's box, by its fit.
#[derive(Resource, Debug)]
struct Tints {
    clear: Handle<StandardMaterial>,
    blocked: Handle<StandardMaterial>,
}

/// The height of the preview's box, which marks the ground the building takes.
const FOOTPRINT_HEIGHT: f32 = 0.5;

/// The key that drops a build.
const DROP_KEY: KeyCode = KeyCode::Escape;

impl Plugin for BuildPreview {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::add_tints);
        app.add_systems(
            Update,
            (
                Self::show,
                (
                    Self::follow,
                    UnitGrid::refresh,
                    Self::judge,
                    Self::tint,
                    Self::click,
                )
                    .chain()
                    .run_if(resource_exists::<Placing>),
            )
                .chain()
                .after(SelectionSystems)
                .after(OrderSystems),
        );
    }
}

impl BuildPreview {
    fn add_tints(
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
        mut commands: Commands<'_, '_>,
    ) {
        let mut tint = |color| {
            materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..StandardMaterial::default()
            })
        };
        commands.insert_resource(Tints {
            clear: tint(Color::srgba(0.3, 0.95, 0.4, 0.4)),
            blocked: tint(Color::srgba(0.95, 0.2, 0.2, 0.4)),
        });
    }

    /// Spawns the preview's drawing as the player starts placing a build, and again for another
    /// build; despawns it as the placing ends, or as its builder leaves the client.
    fn show(
        placing: Option<Res<'_, Placing>>,
        ghost: Option<Single<'_, '_, (Entity, &Ghost)>>,
        index: Res<'_, EntityIndex>,
        (builds, looks): (Res<'_, BuildBook>, Option<Res<'_, UnitLooks>>),
        tints: Res<'_, Tints>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut commands: Commands<'_, '_>,
    ) {
        let placing = placing.map(|placing| *placing);
        if let Some(placing) = placing
            && index.get(placing.builder).is_none()
        {
            commands.remove_resource::<Placing>();
        }
        let shown = ghost.as_deref().map(|&(_, ghost)| ghost.placing);
        if shown == placing {
            return;
        }
        if let Some(&(root, _)) = ghost.as_deref() {
            commands.entity(root).despawn();
        }
        let Some(placing) = placing else {
            return;
        };
        let spec = builds
            .of(placing.action)
            .expect("a build button's action is a build");
        let [a, b] = spec
            .form()
            .at(Num::ZERO)
            .half_edges()
            .expect("a building's body is a box");
        let length = |edge: [Num; 2]| 2.0 * edge[0].float().hypot(edge[1].float());
        let mesh = meshes.add(Cuboid::new(length(a), FOOTPRINT_HEIGHT, length(b)).mesh());
        let footprint = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(tints.clear.clone()),
                Transform::from_xyz(0.0, FOOTPRINT_HEIGHT / 2.0, 0.0),
            ))
            .id();
        let turn = Quat::from_rotation_y(placing.angle.float().to_radians());
        let root = commands
            .spawn((
                Ghost {
                    placing,
                    footprint,
                    at: None,
                    fit: Fit::Clear,
                },
                Transform::from_rotation(turn),
                Visibility::Hidden,
            ))
            .add_child(footprint)
            .id();
        let models = looks
            .as_deref()
            .and_then(|looks| looks.look(placing.building));
        for model in models.unwrap_or_default() {
            commands.spawn((
                WorldAssetRoot(model.scene.clone()),
                model.parts.clone(),
                Transform::from_scale(Vec3::splat(model.scale)),
                ChildOf(root),
            ));
        }
    }

    /// Moves the preview to the ground point under the cursor, as the order sends it: its
    /// height the builder's, as the sim places a building; hidden while the cursor is over the
    /// UI or off the ground. The preview has no shape, which the pointer reads the drawings of.
    fn follow(
        pointer: Pointer<'_, '_>,
        index: Res<'_, EntityIndex>,
        builders: Query<'_, '_, &Position, Allow<Unpredicted>>,
        ghost: Single<'_, '_, (&mut Ghost, &mut Transform, &mut Visibility), Without<Shape>>,
    ) {
        let (mut ghost, mut transform, mut visibility) = ghost.into_inner();
        let ground = pointer.ground().filter(|_| !pointer.over_ui());
        let height = index
            .get(ghost.placing.builder)
            .and_then(|builder| builders.get(builder).ok())
            .map(|pos| pos.get().y);
        let at = ground.zip(height).and_then(|(ground, y)| {
            let x = float_num::to_num(ground.x)?;
            let z = float_num::to_num(ground.z)?;
            Some((ground, SimVec3::new(x, y, z)))
        });
        ghost.at = at.map(|(_, at)| at);
        let Some((ground, at)) = at else {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        };
        transform.translation = Vec3::new(at.x.float(), ground.y, at.z.float());
        visibility.set_if_neq(Visibility::Inherited);
    }

    /// Tests the building's box where the preview stands, as the sim tests its placement for
    /// the builder's team, against what the client holds.
    fn judge(
        index: Res<'_, EntityIndex>,
        builders: Query<'_, '_, &Team, Allow<Unpredicted>>,
        builds: Res<'_, BuildBook>,
        mut placement: Placement<'_, '_, ByHolding>,
        mut ghost: Single<'_, '_, &mut Ghost>,
    ) {
        let placing = ghost.placing;
        let team = index
            .get(placing.builder)
            .and_then(|builder| builders.get(builder).ok());
        let (Some(at), Some(&team)) = (ghost.at, team) else {
            return;
        };
        let spec = builds
            .of(placing.action)
            .expect("a build button's action is a build");
        let body = spec.form().at(placing.angle);
        let fit = if placement.passes(team, at, body, spec) {
            Fit::Clear
        } else {
            Fit::Blocked
        };
        ghost.fit = fit;
    }

    /// Gives the preview's box the tint of its fit.
    fn tint(
        tints: Res<'_, Tints>,
        ghost: Single<'_, '_, &Ghost>,
        mut figures: Query<'_, '_, &mut MeshMaterial3d<StandardMaterial>>,
    ) {
        let tint = match ghost.fit {
            Fit::Clear => &tints.clear,
            Fit::Blocked => &tints.blocked,
        };
        if let Ok(mut figure) = figures.get_mut(ghost.footprint)
            && figure.0 != *tint
        {
            figure.0 = tint.clone();
        }
    }

    /// A left click over the ground, where the building may stand, orders the build there and
    /// ends the placing; a right click or Esc ends it.
    fn click(
        buttons: Res<'_, ButtonInput<MouseButton>>,
        keys: Res<'_, ButtonInput<KeyCode>>,
        pointer: Pointer<'_, '_>,
        ghost: Single<'_, '_, &Ghost>,
        mut orders: ResMut<'_, PendingOrders>,
        mut commands: Commands<'_, '_>,
    ) {
        if buttons.just_pressed(MouseButton::Right) || keys.just_pressed(DROP_KEY) {
            commands.remove_resource::<Placing>();
            return;
        }
        if !buttons.just_pressed(MouseButton::Left) || pointer.over_ui() {
            return;
        }
        let (Some(at), Fit::Clear) = (ghost.at, ghost.fit) else {
            return;
        };
        let Placing {
            builder,
            slot,
            angle,
            ..
        } = ghost.placing;
        let target = BuildTarget::Point {
            x: at.x,
            z: at.z,
            angle,
        };
        orders.push(Order::one(builder, OrdersAction::Build { slot, target }));
        commands.remove_resource::<Placing>();
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::{IntoSystem, RunSystemOnce, System};
    use bevy::ecs::world::World;
    use campfire_actions::BuildStart;
    use campfire_capabilities::Match;
    use campfire_capabilities::internals::ProductionFixtures;
    use campfire_production::{NewBuild, Production, Style};
    use campfire_sim::Capability;
    use campfire_units::BodyForm;
    use campfire_values::{ActionId, Share, UnitType};

    use super::*;

    #[test]
    fn the_previews_systems_take_no_conflicting_access() {
        // Bevy checks that a system's queries do not conflict as it initializes the system.
        let mut world = World::new();
        IntoSystem::into_system(BuildPreview::show).initialize(&mut world);
        IntoSystem::into_system(BuildPreview::follow).initialize(&mut world);
        IntoSystem::into_system(BuildPreview::judge).initialize(&mut world);
        IntoSystem::into_system(BuildPreview::tint).initialize(&mut world);
        IntoSystem::into_system(BuildPreview::click).initialize(&mut world);
    }

    #[test]
    fn the_preview_is_red_over_an_enemys_body_and_green_clear_of_it() {
        // Walkers of a half meter: a builder of team 0 at the origin, an enemy at (10, 0), and a
        // friend at (−10, 0).
        let mut game = Match::predicting(&[Capability::Navigation, Capability::Production]);
        let (own, enemy) = (Team::new(0), Team::new(1));
        let builder = Production::yard_walker(&mut game, (0, 0, 0), own, None);
        Production::yard_walker(&mut game, (10, 0, 0), enemy, None);
        Production::yard_walker(&mut game, (-10, 0, 0), own, None);
        // A build of a box of 4 by 2 m.
        let action = ActionId::new(0);
        let mut builds = BuildBook::default();
        let build = NewBuild {
            form: BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap(),
            start: BuildStart::Order,
            style: Style::Builder,
            start_life: None,
            refund: Share::ALL,
            near: Vec::new(),
            away: Vec::new(),
        };
        builds.push(action, build);
        game.insert_resource(builds);
        let mut fit = |x: i64, angle: i64| {
            let placing = Placing {
                builder,
                slot: 0,
                action,
                building: UnitType::new(0),
                angle: Num::int(angle),
            };
            let footprint = game.spawn_empty().id();
            let ghost = Ghost {
                placing,
                footprint,
                at: Some(SimVec3::new(Num::int(x), Num::ZERO, Num::ZERO)),
                fit: Fit::Clear,
            };
            let ghost = game.spawn(ghost).id();
            game.run_system_once(UnitGrid::refresh).unwrap();
            game.run_system_once(BuildPreview::judge).unwrap();
            let fit = game.get::<Ghost>(ghost).unwrap().fit;
            game.despawn(ghost);
            fit
        };
        // Its box reaches 2 m along x and 1 m along z, unturned: at (10, 0) it lies over the
        // enemy, and at (5, 0) it reaches x = 7, short of the enemy's edge at 9.5. At (8, 0) it
        // reaches 10, past that edge; turned a quarter, its 1 m half lies along x and reaches 9,
        // short of it. Over the friend, at (−10, 0), it may stand.
        assert_eq!(fit(10, 0), Fit::Blocked);
        assert_eq!(fit(5, 0), Fit::Clear);
        assert_eq!(fit(8, 0), Fit::Blocked);
        assert_eq!(fit(8, 90), Fit::Clear);
        assert_eq!(fit(-10, 0), Fit::Clear);
    }
}
