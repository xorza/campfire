use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::{Allow, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::common_conditions::{resource_exists_and_changed, resource_removed};
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemCondition};
use bevy::ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::material::AlphaMode;
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::shape::{Cuboid, Cylinder};
use bevy::state::condition::in_state;
use bevy::state::state::{NextState, OnExit};
use bevy::transform::components::Transform;
use campfire_capabilities::Order;
use campfire_math::{Num, Vec3 as SimVec3};
use campfire_net::PendingOrders;
use campfire_orders::OrdersDirective;
use campfire_production::{BuildBook, Placement};
use campfire_sim::{EntityIndex, Position, Unpredicted};
use campfire_units::{BuildTarget, GroundPoint, UnitGrid};
use campfire_values::Team;
use campfire_vision::ByHolding;

use crate::input_map::{Act, InputMap};
use crate::input_mode::InputMode;
use crate::orders::OrderSystems;
use crate::placing::Placing;
use crate::pointer::Pointer;
use crate::selection::SelectionSystems;
use crate::view::drawing::DrawingOf;
use crate::view::float_num::{self, FloatNum};
use crate::view::match_colors::{Paint, Tint};
use crate::view::unit_looks::UnitLooks;

/// The build preview (02-engine-core.md, A command panel from the actions): while the player places
/// a build, its building's look follows the ground point under the cursor, at its type's angle,
/// over its footprint drawn green where it may stand and red where it overlaps a wall or a body the
/// client holds; a left click on green orders the build there, and a right click or the key map's
/// drop chord, Esc of the engine's, drops it. It takes the clicks in `InputMode::Place`, which the
/// selection and the orders leave to it, and ends the placing as the mode leaves.
#[derive(Debug)]
pub(crate) struct BuildPreviewPlugin;

/// The root of the preview's drawing: the build it shows, its footprint's figure, the point it stands
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

impl Fit {
    /// The paint of the preview's look for the builder's `team`: tinted red where the building
    /// may not stand, as Zero Hour tints its placement's drawable.
    fn paint(self, team: Option<Team>) -> Paint {
        Paint {
            team,
            tint: (self == Self::Blocked).then_some(Tint::Blocked),
        }
    }
}

/// The see-through tints of the preview's footprint, by its fit.
#[derive(Resource, Debug)]
struct Tints {
    clear: Handle<StandardMaterial>,
    blocked: Handle<StandardMaterial>,
}

/// The height of the preview's footprint, which marks the ground the building takes.
const FOOTPRINT_HEIGHT: f32 = 0.5;

impl Plugin for BuildPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::add_tints);
        app.add_systems(OnExit(InputMode::Place), Self::end_placing);
        app.add_systems(
            Update,
            (
                Self::show.run_if(
                    resource_exists_and_changed::<Placing>.or_eager(resource_removed::<Placing>),
                ),
                (
                    Self::drop_lost,
                    Self::follow,
                    UnitGrid::refresh,
                    Self::judge,
                    Self::tint,
                    Self::click,
                )
                    .chain()
                    .run_if(in_state(InputMode::Place)),
            )
                .chain()
                .after(SelectionSystems)
                .after(OrderSystems),
        );
    }
}

impl BuildPreviewPlugin {
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

    /// Ends the placing as its builder leaves the client.
    fn drop_lost(
        placing: Res<'_, Placing>,
        index: Res<'_, EntityIndex>,
        mut mode: ResMut<'_, NextState<InputMode>>,
    ) {
        if index.get(placing.builder).is_none() {
            mode.set_if_different(InputMode::Select);
        }
    }

    /// Spawns the preview's drawing as the player starts placing a build, and again for another
    /// build; despawns it as the placing ends.
    fn show(
        placing: Option<Res<'_, Placing>>,
        ghost: Option<Single<'_, '_, (Entity, &Ghost)>>,
        (builds, looks): (Res<'_, BuildBook>, Res<'_, UnitLooks>),
        tints: Res<'_, Tints>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut commands: Commands<'_, '_>,
    ) {
        let placing = placing.map(|placing| *placing);
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
        let form = spec.form();
        let mesh = if let Some(radius) = form.circle_radius() {
            Mesh::from(Cylinder::new(radius.float(), FOOTPRINT_HEIGHT))
        } else {
            let [a, b] = form
                .at(Num::ZERO)
                .half_edges()
                .expect("a building's body is a box or a circle");
            let length = |edge: [Num; 2]| 2.0 * edge[0].float().hypot(edge[1].float());
            Mesh::from(Cuboid::new(length(a), FOOTPRINT_HEIGHT, length(b)))
        };
        let mesh = meshes.add(mesh);
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
        for model in looks.look(placing.building).unwrap_or_default() {
            commands.spawn((model.bundle(Paint::default()), ChildOf(root)));
        }
    }

    /// Moves the preview to the ground point under the cursor, as the order sends it: its
    /// height the builder's, as the sim places a building; hidden while the cursor is over the
    /// UI or off the ground. The preview has no shape, which the pointer reads the drawings of.
    fn follow(
        pointer: Pointer<'_, '_>,
        index: Res<'_, EntityIndex>,
        builders: Query<'_, '_, &Position, Allow<Unpredicted>>,
        ghost: Single<'_, '_, (&mut Ghost, &mut Transform, &mut Visibility), Without<DrawingOf>>,
    ) {
        let (mut ghost, mut transform, mut visibility) = ghost.into_inner();
        let ground = pointer.ground().filter(|_| !pointer.is_over_ui());
        let height = index
            .get(ghost.placing.builder)
            .and_then(|builder| builders.get(builder).ok())
            .map(|position| position.get().y);
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

    /// Tests the building's body where the preview stands, as the sim tests its placement for
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

    /// Gives the preview's footprint the tint of its fit, and its look the builder's team and, where
    /// the building may not stand, the blocked tint.
    fn tint(
        tints: Res<'_, Tints>,
        (index, builders): (
            Res<'_, EntityIndex>,
            Query<'_, '_, &Team, Allow<Unpredicted>>,
        ),
        ghost: Single<'_, '_, (&Ghost, &Children)>,
        mut figures: Query<'_, '_, &mut MeshMaterial3d<StandardMaterial>>,
        mut paints: Query<'_, '_, &mut Paint>,
    ) {
        let (ghost, models) = ghost.into_inner();
        let tint = match ghost.fit {
            Fit::Clear => &tints.clear,
            Fit::Blocked => &tints.blocked,
        };
        if let Ok(mut figure) = figures.get_mut(ghost.footprint)
            && figure.0 != *tint
        {
            figure.0 = tint.clone();
        }
        let team = index
            .get(ghost.placing.builder)
            .and_then(|builder| builders.get(builder).ok())
            .copied();
        let paint = ghost.fit.paint(team);
        let mut models = paints.iter_many_mut(models);
        while let Some(model) = models.fetch_next() {
            if let Ok(mut model) = model {
                model.set_if_neq(paint);
            }
        }
    }

    /// A left click over the ground, where the building may stand, orders the build there and
    /// ends the placing; a right click or a chord that drops the build ends it.
    fn click(
        buttons: Res<'_, ButtonInput<MouseButton>>,
        (keys, input): (Res<'_, ButtonInput<KeyCode>>, Res<'_, InputMap>),
        pointer: Pointer<'_, '_>,
        ghost: Single<'_, '_, &Ghost>,
        mut orders: ResMut<'_, PendingOrders>,
        mut mode: ResMut<'_, NextState<InputMode>>,
    ) {
        if buttons.just_pressed(MouseButton::Right) || input.pressed(Act::DropBuild, &keys) {
            mode.set_if_different(InputMode::Select);
            return;
        }
        if !buttons.just_pressed(MouseButton::Left) || pointer.is_over_ui() {
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
            at: GroundPoint { x: at.x, z: at.z },
            angle,
        };
        orders.push(Order::one(builder, OrdersDirective::Build { slot, target }));
        mode.set_if_different(InputMode::Select);
    }

    /// Ends the placing as the input's mode leaves `InputMode::Place`, so the preview's drawing
    /// goes.
    fn end_placing(mut commands: Commands<'_, '_>) {
        commands.remove_resource::<Placing>();
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::state::app::{AppExtStates, StatesPlugin};
    use bevy::state::state::State;
    use campfire_capabilities::Match;
    use campfire_capabilities::internals::ProductionFixtures;
    use campfire_declarations::Role;
    use campfire_production::{BuildStart, NewBuild, Production, Style};
    use campfire_sim::{Capability, IdAllocator};
    use campfire_units::BodyForm;
    use campfire_values::{ActionId, Share, UnitType};

    use super::*;

    #[test]
    fn the_placing_lasts_while_the_input_places_and_goes_as_it_selects_again() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<InputMode>();
        app.add_systems(OnExit(InputMode::Place), BuildPreviewPlugin::end_placing);
        let placing = Placing {
            builder: IdAllocator::default().allocate(),
            slot: 0,
            action: ActionId::new(0),
            building: UnitType::new(0),
            angle: Num::ZERO,
        };
        let mode = |app: &App| *app.world().resource::<State<InputMode>>().get();
        let set = |app: &mut App, mode| {
            let mut next = app.world_mut().resource_mut::<NextState<InputMode>>();
            next.set_if_different(mode);
        };
        app.insert_resource(placing);
        set(&mut app, InputMode::Place);
        app.update();
        assert_eq!(mode(&app), InputMode::Place);
        // Another build placed keeps the mode, and its placing with it.
        set(&mut app, InputMode::Place);
        app.update();
        assert!(app.world().contains_resource::<Placing>());
        set(&mut app, InputMode::Select);
        app.update();
        assert_eq!(mode(&app), InputMode::Select);
        assert!(!app.world().contains_resource::<Placing>());
    }

    #[test]
    fn the_preview_is_red_over_an_enemys_body_and_green_clear_of_it() {
        // Walkers of a half meter: a builder of team 0 at the origin, an enemy at (10, 0), and a
        // friend at (−10, 0).
        let mut game = Match::builder(&[Capability::Navigation, Capability::Production])
            .role(Role::Client)
            .build();
        let (own, enemy) = (Team::new(0), Team::new(1));
        let builder = Production::yard_walker(&mut game, (0, 0, 0), own, None);
        Production::yard_walker(&mut game, (10, 0, 0), enemy, None);
        Production::yard_walker(&mut game, (-10, 0, 0), own, None);
        // A build of a box of 4 by 2 m, and one of a circle of 2 m.
        let (action, round) = (ActionId::new(0), ActionId::new(1));
        let mut builds = BuildBook::default();
        let forms = [
            BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap(),
            BodyForm::circle(Num::int(2)).unwrap(),
        ];
        for (id, form) in [action, round].into_iter().zip(forms) {
            let build = NewBuild {
                unit_type: UnitType::new(0),
                form,
                start: BuildStart::Order,
                style: Style::Builder,
                start_life: None,
                refund: Share::ALL,
                near: Vec::new(),
                away: Vec::new(),
            };
            builds.push(id, build);
        }
        game.insert_resource(builds);
        let mut fit_of = |action: ActionId, x: i64, angle: i64| {
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
            game.run_system_once(BuildPreviewPlugin::judge).unwrap();
            let fit = game.get::<Ghost>(ghost).unwrap().fit;
            game.despawn(ghost);
            fit
        };
        let mut fit = |x: i64, angle: i64| fit_of(action, x, angle);
        // Its box reaches 2 m along x and 1 m along z, unturned: at (10, 0) it lies over the
        // enemy, and at (5, 0) it reaches x = 7, short of the enemy's edge at 9.5. At (8, 0) it
        // reaches 10, past that edge; turned a quarter, its 1 m half lies along x and reaches 9,
        // short of it. Over the friend, at (−10, 0), it may stand.
        assert_eq!(fit(10, 0), Fit::Blocked);
        assert_eq!(fit(5, 0), Fit::Clear);
        assert_eq!(fit(8, 0), Fit::Blocked);
        assert_eq!(fit(8, 90), Fit::Clear);
        assert_eq!(fit(-10, 0), Fit::Clear);
        // The circle reaches 2 m every way: at (7, 0) it reaches x = 9, short of the enemy's edge,
        // and at (8, 0) 10, past it, at any angle.
        assert_eq!(fit_of(round, 7, 0), Fit::Clear);
        assert_eq!(fit_of(round, 8, 90), Fit::Blocked);
        // Its look takes the builder's team, and red where it may not stand.
        let team = Some(Team::new(1));
        assert_eq!(
            [Fit::Clear.paint(team), Fit::Blocked.paint(team)],
            [
                Paint { team, tint: None },
                Paint {
                    team,
                    tint: Some(Tint::Blocked)
                }
            ]
        );
    }
}
