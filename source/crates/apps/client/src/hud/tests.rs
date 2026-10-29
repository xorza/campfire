use std::sync::Arc;

use bevy::ecs::world::World;
use campfire_actions::internals::TestAction;
use campfire_actions::{
    ActionKind, ActionRange, Actions, Aim, SlotKind, SlotKindBook, SlotKindData, SlotKinds,
};
use campfire_client_files::HudFile;
use campfire_common::DeclaredName;
use campfire_common::Ticks;
use campfire_math::Num;
use campfire_script::MatchNames;
use campfire_sim::{IdAllocator, Position};
use campfire_units::{Avatar, BodyUnit};
use campfire_values::{ActionId, UnitType};

use bevy::picking::hover::HoverMap;

use super::*;
use crate::pointer::unit_picking::UnderMouse;
use crate::test_app;
use crate::view::ViewPlugin;

#[test]
fn a_ring_marks_each_drop_of_life_where_the_unit_is_drawn() {
    let mut app = test_app::view();
    // One kind of slot, whose ranks come with the spawn.
    let kind = SlotKindData {
        name: DeclaredName::new("ability").unwrap(),
        ranks: None,
    };
    let kinds = SlotKinds::new([kind]).unwrap();
    app.insert_resource(SlotKindBook(Arc::new(kinds)));
    // Two casts of one rank, the actions 0 and 1 of the avatar's slots.
    app.init_resource::<MatchNames>();
    let range = ActionRange::meters(Num::ZERO);
    let cast = || TestAction::new(ActionKind::Cast, Aim::None, range, Ticks::ZERO);
    for name in ["first", "second"] {
        Actions::add(app.world_mut(), name, cast());
    }
    app.add_plugins((ViewPlugin, HudPlugin));
    app.update();
    assert!(app.world().contains_resource::<HudPalette>());
    // With no own avatar there is no target to mark: nothing wears the target's material.
    let target = app.world().resource::<HudPalette>().target.clone();
    let mut worn = app.world_mut().query::<&MeshMaterial3d<StandardMaterial>>();
    assert!(worn.iter(app.world()).all(|material| material.0 != target));

    // A unit of 600 life at (3, 0, −2), whose life gauge showed it full.
    let life = |max| Life::full(Num::int(max));
    let position = Position::from((3, 0, -2));
    let unit = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            BodyUnit,
            position,
            Team::new(0),
            // A type the client data draws with no model, as every type of a view with no
            // looks.
            UnitType::new(0),
            life(600),
        ))
        .id();
    app.update();
    let root = app.world().get::<Drawing>(unit).unwrap().root();
    let shown = |shown| GaugeKind::Life {
        shown: Some(Num::int(shown)),
    };
    let stand = app
        .world_mut()
        .spawn((GaugeStand, Transform::default(), ChildOf(root)))
        .id();
    let gauge = app
        .world_mut()
        .spawn((
            Gauge {
                kind: shown(600),
                fill: Entity::PLACEHOLDER,
            },
            ChildOf(stand),
        ))
        .id();
    app.world_mut().entity_mut(root).insert(Gauged {
        stand: Some(stand),
        slots: None,
        badge: None,
    });
    let mut rings = app.world_mut().query::<(&Ring, &Transform)>();
    // Unchanged, its life marks nothing; a drop to 450 marks one ring on the ground under it,
    // a rise to 500 none, and the gauge keeps what it showed last.
    app.update();
    assert_eq!(rings.iter(app.world()).count(), 0);
    app.world_mut().entity_mut(unit).insert(life(450));
    app.update();
    let (_, ring) = rings.single(app.world()).unwrap();
    assert_eq!(ring.translation, Vec3::new(3.0, 0.05, -2.0));
    let reach = app.world().get::<UnitShape>(root).unwrap().reach();
    assert_eq!(ring.scale, Vec3::splat(reach));
    assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(450));
    app.world_mut().entity_mut(unit).insert(life(500));
    app.update();
    assert_eq!(app.world().get::<Gauge>(gauge).unwrap().kind, shown(500));
    assert_eq!(rings.iter(app.world()).count(), 1);
    // A mode whose HUD file hides the hit mark marks no drop, to 300.
    let hidden = HudFile {
        hit_mark: HitMark::Hidden,
        ..HudFile::default()
    };
    app.insert_resource(MatchHud(hidden));
    app.world_mut().entity_mut(unit).insert(life(300));
    app.update();
    assert_eq!(rings.iter(app.world()).count(), 1);

    // As the own avatar, its gauges made for one slot of no ranks and no charges: a change of
    // its slots that keeps those rows keeps them, and a second slot takes them down for
    // `add_gauges`, stand and all.
    let slot = |at| (ActionId::new(at), SlotKind::new(0), Some(Rank::FIRST));
    let none = SlotRows {
        ranks: None,
        charges: None,
    };
    app.world_mut().entity_mut(root).insert(Gauged {
        stand: Some(stand),
        slots: Some(vec![none]),
        badge: None,
    });
    app.world_mut()
        .entity_mut(unit)
        .insert(ActionSlots::new([slot(0)]));
    app.update();
    app.world_mut()
        .entity_mut(unit)
        .insert(ActionSlots::new([slot(1)]));
    app.update();
    assert!(app.world().get::<Gauged>(root).is_some());
    app.world_mut()
        .entity_mut(unit)
        .insert(ActionSlots::new([slot(0), slot(1)]));
    app.update();
    assert!(app.world().get::<Gauged>(root).is_none());
    assert!(app.world().get_entity(stand).is_err());
}

#[test]
fn a_selected_unit_is_ringed_at_its_reach_once_drawn_until_it_leaves_the_selection() {
    let mut app = test_app::view();
    app.add_plugins((ViewPlugin, HudPlugin));
    app.update();
    let mut ids = IdAllocator::default();
    let team = Team::new(0);
    let [tower_id, hero_id] = [ids.allocate(), ids.allocate()];
    let tower = app
        .world_mut()
        .spawn((
            tower_id,
            BodyUnit,
            Position::from((0, 0, 0)),
            team,
            UnitType::new(0),
        ))
        .id();
    // The rings under `unit`'s drawing: the scale of each.
    let rings = |world: &World, unit: Entity| -> Vec<Vec3> {
        let ring = &world.resource::<HudPalette>().unit_ring;
        let root = world.get::<Drawing>(unit).unwrap().root();
        world
            .get::<Children>(root)
            .unwrap()
            .iter()
            .copied()
            .filter(|&child| {
                world
                    .get::<Mesh3d>(child)
                    .is_some_and(|mesh| mesh.0 == *ring)
            })
            .map(|child| {
                let transform = world.get::<Transform>(child).unwrap();
                assert_eq!(transform.translation, Vec3::Y * SELECTED_LIFT);
                transform.scale
            })
            .collect()
    };
    // The tower and the hero, not held yet, selected: the tower, of no owner and no walk, is a
    // structure of reach 0.9 m, ringed as it is drawn.
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[tower_id, hero_id], |_| None);
    app.update();
    assert_eq!(rings(app.world(), tower), [Vec3::splat(0.9)]);
    // The hero arrives, a player's avatar, of reach 0.5 m: ringed as it is drawn, with no
    // change of the selection.
    let hero = app
        .world_mut()
        .spawn((
            hero_id,
            BodyUnit,
            Position::from((4, 0, 0)),
            team,
            UnitType::new(0),
            Avatar,
        ))
        .id();
    app.update();
    assert_eq!(rings(app.world(), hero), [Vec3::splat(0.5)]);
    assert_eq!(rings(app.world(), tower).len(), 1);
    // The hero alone selected: the tower's ring goes, the hero keeps its one.
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[hero_id], |_| None);
    app.update();
    assert_eq!(rings(app.world(), tower), []);
    assert_eq!(rings(app.world(), hero), [Vec3::splat(0.5)]);

    // A leader of two living spawns selected: each spawn is ringed, at the tower's reach as each
    // is a structure as it stands, and the leader not (02-engine-core.md, A member picks as its
    // leader).
    let [leader_id, first_id, second_id] = [ids.allocate(), ids.allocate(), ids.allocate()];
    let mut living = LivingSpawns::with_capacity(2);
    living.push(first_id);
    living.push(second_id);
    let unit = |id, x: i32| {
        (
            id,
            BodyUnit,
            Position::from((x, 0, 8)),
            team,
            UnitType::new(0),
        )
    };
    let leader = app
        .world_mut()
        .spawn((unit(leader_id, 0), Leader, living))
        .id();
    let [first, second] =
        [(first_id, -2), (second_id, 2)].map(|(id, x)| app.world_mut().spawn(unit(id, x)).id());
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[leader_id], |_| None);
    app.update();
    assert_eq!(rings(app.world(), first), [Vec3::splat(0.9)]);
    assert_eq!(rings(app.world(), second), [Vec3::splat(0.9)]);
    assert_eq!(rings(app.world(), leader), []);
    assert_eq!(rings(app.world(), hero), []);
    // The leader and its first spawn both selected: the spawn is ringed once.
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[leader_id, first_id], |_| None);
    app.update();
    assert_eq!(rings(app.world(), first), [Vec3::splat(0.9)]);
}

#[test]
fn a_unit_shows_its_bars_while_selected_when_the_hud_shows_the_selected_alone() {
    // Two units, each gauged on a stand, and none under the mouse, so the selection alone shows
    // bars: the first's, then the second's; then a leader's, its two living spawns'; then the
    // second's drawing, torn off its unit, takes its gauges down.
    let mut app = test_app::view();
    app.init_resource::<HoverMap>();
    app.init_resource::<UnderMouse>();
    app.init_resource::<MarkedUnits>();
    app.add_systems(
        Update,
        (
            HudPlugin::mark_selected,
            HudPlugin::take_down_torn,
            HudPlugin::show_selected_bars,
        )
            .chain(),
    );
    let world = app.world_mut();
    world.spawn((Window::default(), PrimaryWindow));
    world.spawn(Camera3d::default());
    let mut ids = IdAllocator::default();
    let [first, second, led_first, led_second] = [(); 4].map(|()| {
        let id = ids.allocate();
        let unit = world.spawn(id).id();
        let stand = world.spawn((GaugeStand, Visibility::Inherited)).id();
        let root = world
            .spawn((
                DrawingOf(unit),
                Gauged {
                    stand: Some(stand),
                    slots: None,
                    badge: None,
                },
            ))
            .id();
        (id, stand, root)
    });
    let mut living = LivingSpawns::with_capacity(2);
    living.push(led_first.0);
    living.push(led_second.0);
    let leader = ids.allocate();
    world.spawn((leader, Leader, living));
    let shown = |app: &App| {
        [first, second, led_first, led_second].map(|(_, stand, _)| {
            *app.world().get::<Visibility>(stand).unwrap() == Visibility::Inherited
        })
    };
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[first.0], |_| None);
    app.update();
    assert_eq!(shown(&app), [true, false, false, false]);
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[leader], |_| None);
    app.update();
    assert_eq!(shown(&app), [false, false, true, true]);
    app.world_mut()
        .resource_mut::<Selection>()
        .pick(&[second.0], |_| None);
    app.update();
    assert_eq!(shown(&app), [false, true, false, false]);
    let (_, stand, root) = second;
    app.world_mut().entity_mut(root).remove::<DrawingOf>();
    app.update();
    let world = app.world();
    assert!(world.get_entity(stand).is_err());
    assert!(world.get::<Gauged>(root).is_none());
    assert!(world.get::<Gauged>(first.2).is_some());
}

#[test]
fn a_stand_faces_the_camera_in_the_world_whatever_its_root_turns() {
    // A camera turned 0.5 rad about y, over a root its unit turned 90° 10 m before it: the
    // stand's turn in the world, the root's then its own, is the camera's facing, each part
    // within the rounding of three products of quaternions, and its scale a pixel at 10 m of
    // the default view, 45° high, on a window 720 pixels high, 2 · 10 · tan 22.5° / 720 m. A
    // frame that changes neither turns it none, nor one whose cursor moves, and a turn of its
    // root turns it again.
    let mut app = App::new();
    app.add_systems(Update, HudPlugin::face_camera);
    let facing = Quat::from_rotation_y(0.5) * Quat::from_rotation_x(FRAC_PI_2);
    let world = app.world_mut();
    world.spawn((
        Camera3d::default(),
        Projection::default(),
        Transform::from_rotation(Quat::from_rotation_y(0.5)),
    ));
    world.spawn((Window::default(), PrimaryWindow));
    let turn = Quat::from_rotation_y(FRAC_PI_2);
    let ahead = Quat::from_rotation_y(0.5) * Vec3::new(0.0, 0.0, -10.0);
    let root = world
        .spawn(Transform::from_translation(ahead).with_rotation(turn))
        .id();
    let stand = world
        .spawn((GaugeStand, Transform::default(), ChildOf(root)))
        .id();
    let faces = |app: &App, turn: Quat| {
        let own = app.world().get::<Transform>(stand).unwrap().rotation;
        (turn * own).abs_diff_eq(facing, 1e-6)
    };
    app.update();
    assert!(faces(&app, turn));
    let pixel = 20.0 * std::f32::consts::FRAC_PI_8.tan() / 720.0;
    let scale = app.world().get::<Transform>(stand).unwrap().scale;
    assert!(scale.abs_diff_eq(Vec3::splat(pixel), 1e-6), "{scale}");
    app.world_mut()
        .get_mut::<Transform>(stand)
        .unwrap()
        .rotation = Quat::IDENTITY;
    app.update();
    assert_eq!(
        app.world().get::<Transform>(stand).unwrap().rotation,
        Quat::IDENTITY
    );
    let mut windows = app.world_mut().query::<&mut Window>();
    let mut window = windows.single_mut(app.world_mut()).unwrap();
    window.set_cursor_position(Some(Vec2::new(10.0, 20.0)));
    app.update();
    assert_eq!(
        app.world().get::<Transform>(stand).unwrap().rotation,
        Quat::IDENTITY
    );
    let turn = Quat::from_rotation_y(-1.0);
    app.world_mut().get_mut::<Transform>(root).unwrap().rotation = turn;
    app.update();
    assert!(faces(&app, turn));
}

#[test]
fn an_avatars_ability_rows_stack_its_marks_cooldowns_ranks_and_charges() {
    // Three slots: one of 3 ranks; one of 2 charges; one of neither. Row 1 holds the learn
    // mark over the ranked slot, row 2 a cooldown pip for each slot, row 3 the ranked slot's
    // 3 ticks, row 4 the charged slot's 2.
    let slots = [
        SlotRows {
            ranks: Some(3),
            charges: None,
        },
        SlotRows {
            ranks: None,
            charges: Some(2),
        },
        SlotRows {
            ranks: None,
            charges: None,
        },
    ];
    let mut scratch = GaugeScratch {
        slots: slots.to_vec(),
        ..GaugeScratch::default()
    };
    scratch.ability_rows(1);
    // Each of the three slots' gauges knows the row of three it centers on.
    let at = |slot| SlotPlace { slot, of: 3 };
    let cooldown = |slot| GaugeKind::Cooldown {
        at: at(slot),
        row: 2,
        cooling: None,
    };
    let rank = |rank| GaugeKind::Rank {
        at: at(0),
        rank,
        ranks: 3,
        row: 3,
    };
    let charge = |charge| GaugeKind::Charge {
        at: at(1),
        charge,
        max: 2,
        row: 4,
    };
    assert_eq!(
        scratch.kinds,
        [
            GaugeKind::Learnable { at: at(0), row: 1 },
            cooldown(0),
            cooldown(1),
            cooldown(2),
            rank(1),
            rank(2),
            rank(3),
            charge(1),
            charge(2),
        ]
    );
    // With no ability that ranks up, the cooldowns take the first row and the charges the
    // next; the two slots center on a row of two.
    let mut scratch = GaugeScratch {
        slots: slots[1..].to_vec(),
        ..GaugeScratch::default()
    };
    scratch.ability_rows(1);
    let at = |slot| SlotPlace { slot, of: 2 };
    let cooldown = |slot| GaugeKind::Cooldown {
        at: at(slot),
        row: 1,
        cooling: None,
    };
    let charge = |charge| GaugeKind::Charge {
        at: at(0),
        charge,
        max: 2,
        row: 2,
    };
    assert_eq!(
        scratch.kinds,
        [cooldown(0), cooldown(1), charge(1), charge(2)]
    );
}

#[test]
fn a_unit_shows_the_badge_of_its_level_from_2_by_its_stand_and_none_at_level_1() {
    // A unit drawn, its root gauged with a badge on its stand, and two levels' materials, of
    // levels 2 and 3: at level 1 the badge is hidden; at 2 it shows the first, at 3 the second,
    // and at 4, past the last, none; dead, none.
    let mut app = test_app::view();
    app.add_plugins(ViewPlugin);
    app.add_systems(
        Update,
        HudPlugin::show_levels
            .run_if(resource_exists::<LevelBadges>)
            .after(ViewSystems),
    );
    let unit = app
        .world_mut()
        .spawn((
            IdAllocator::default().allocate(),
            BodyUnit,
            Position::from((0, 0, 0)),
            Team::new(0),
            UnitType::new(0),
            Life::full(Num::int(100)),
            Level::new(1).unwrap(),
        ))
        .id();
    app.update();
    let root = app.world().get::<Drawing>(unit).unwrap().root();
    let stand = app
        .world_mut()
        .spawn((GaugeStand, Transform::default(), ChildOf(root)))
        .id();
    let materials = [(); 2].map(|()| {
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default())
    });
    let badge = app
        .world_mut()
        .spawn((
            LevelBadge,
            MeshMaterial3d(Handle::<StandardMaterial>::default()),
            Layout::badge(),
            Visibility::Hidden,
            ChildOf(stand),
        ))
        .id();
    app.world_mut().insert_resource(LevelBadges(
        materials
            .iter()
            .map(|material| BadgeMaterial {
                image: Handle::default(),
                rect: Rect::new(0.0, 0.0, 16.0, 16.0),
                material: material.clone(),
                fitted: true,
            })
            .collect(),
    ));
    app.world_mut().entity_mut(root).insert(Gauged {
        stand: Some(stand),
        slots: None,
        badge: Some(badge),
    });
    app.update();
    let shown = |app: &App| {
        let world = app.world();
        let visible = *world.get::<Visibility>(badge).unwrap() == Visibility::Inherited;
        let material = &world
            .get::<MeshMaterial3d<StandardMaterial>>(badge)
            .unwrap()
            .0;
        visible.then(|| materials.iter().position(|held| held == material))
    };
    assert_eq!(shown(&app), None);
    // The badge stands left of the life bar, (64 / 2 + 2 + 12 / 2) = 40 to the left, at its
    // height, 12 square.
    let placed = *app.world().get::<Transform>(badge).unwrap();
    assert_eq!(placed.translation.x, -40.0);
    assert_eq!(placed.scale, Vec3::new(12.0, 1.0, 12.0));
    for (level, expected) in [(2, Some(Some(0))), (3, Some(Some(1))), (4, None)] {
        *app.world_mut().get_mut::<Level>(unit).unwrap() = Level::new(level).unwrap();
        app.update();
        assert_eq!(shown(&app), expected, "level {level}");
    }
    *app.world_mut().get_mut::<Level>(unit).unwrap() = Level::new(2).unwrap();
    app.world_mut().entity_mut(unit).insert(Dead);
    app.update();
    assert_eq!(shown(&app), None);
    // A rectangle of a texture, as its share: (16, 0) to (32, 16) of 64 by 32 scales by a
    // quarter and a half, and moves a quarter across.
    let uv = badge_uv(Rect::new(16.0, 0.0, 32.0, 16.0), Vec2::new(64.0, 32.0));
    assert_eq!(
        uv,
        Affine2::from_scale_angle_translation(Vec2::new(0.25, 0.5), 0.0, Vec2::new(0.25, 0.0))
    );
}
