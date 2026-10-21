use std::f32::consts::FRAC_PI_2;

use bevy::camera::PerspectiveProjection;
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::world::World;
use bevy::math::{Dir3, Vec2};
use bevy::picking::hover::HoverMap;
use bevy::transform::components::Transform;
use bevy::ui::{Node, Val};
use bevy::window::{CursorMoved, PrimaryWindow, Window, WindowEvent, WindowResolution};
use campfire_sim::{IdAllocator, StableId};
use campfire_values::Team;

use super::*;
use crate::pointer::Pointer;
use crate::test_app;
use crate::view::footing::Footing;

/// An upright cylinder of `radius`, 2 m tall.
const fn post(radius: f32) -> UnitShape {
    UnitShape {
        radius,
        height: 2.0,
        footing: Footing::Circle(radius),
    }
}

#[test]
fn the_hits_rank_by_tier_then_depth_then_id_and_a_least_radius_widens_a_small_unit() {
    // A camera at (0, 1, 10) facing −z, of a view 90° high on a screen 720 pixels high: at a
    // depth of 10 m a pixel spans 2 · 10 · tan 45° / 720 = 1/36 m.
    let eye = Transform::from_xyz(0.0, 1.0, 10.0);
    let fov = Projection::Perspective(PerspectiveProjection {
        fov: FRAC_PI_2,
        ..PerspectiveProjection::default()
    });
    let scale = PixelScale::of(eye, &fov, 720.0);
    let mut world = World::new();
    let mut ids = IdAllocator::default();
    let mut target = |tier: u32, x: f32, z: f32, shape: UnitShape| Target {
        root: world.spawn_empty().id(),
        unit: Pointed {
            id: ids.allocate(),
            team: Team::new(0),
            owner: None,
        },
        tier,
        base: Vec3::new(x, 0.0, z),
        shape,
    };
    // Along the level ray from (0, 1, 10) to −z, posts of radius 0.5: one of tier 1 at the
    // origin, met at 9.5; two of tier 0 at z = −5, met at 14.5, the second made first; one of
    // tier 0 at z = −10, met at 19.5; and one 3 m to the side, which the ray misses.
    let near = target(1, 0.0, 0.0, post(0.5));
    let mut twins = [
        target(0, 0.0, -5.0, post(0.5)),
        target(0, 0.0, -5.0, post(0.5)),
    ];
    twins.swap(0, 1);
    let far = target(0, 0.0, -10.0, post(0.5));
    let aside = target(0, 3.0, 0.0, post(0.5));
    let ray = Ray3d::new(Vec3::new(0.0, 1.0, 10.0), Dir3::NEG_Z);
    let picking = UnitPicking::default();
    let mut hits = Vec::new();
    picking.hits(
        ray,
        &scale,
        &[near, twins[0], aside, far, twins[1]],
        &mut hits,
    );
    let ranked = |hits: &[UnitHit]| -> Vec<_> {
        hits.iter()
            .map(|hit| (hit.unit.id, hit.tier, hit.depth))
            .collect()
    };
    // The twins are of tier 0, above the nearer post of tier 1, the lower id first.
    assert_eq!(
        ranked(&hits),
        [
            (twins[1].unit.id, 0, 14.5),
            (twins[0].unit.id, 0, 14.5),
            (far.unit.id, 0, 19.5),
            (near.unit.id, 1, 9.5),
        ]
    );

    // A post of radius 0.1 at the origin, its center at a depth of 10 m, and a level ray 0.2 m
    // to its side: missed by its own radius, met when 10 pixels widen it to 10/36 = 5/18 m,
    // where x = 0.2, z = √((5/18)² − 0.2²) = √301 / 90, 10 − √301 / 90 along.
    let thin = target(0, 0.0, 0.0, post(0.1));
    let beside = Ray3d::new(Vec3::new(0.2, 1.0, 10.0), Dir3::NEG_Z);
    picking.hits(beside, &scale, &[thin], &mut hits);
    assert_eq!(ranked(&hits), []);
    let widened = UnitPicking {
        least_radius_px: 10.0,
    };
    widened.hits(beside, &scale, &[thin], &mut hits);
    let want = 10.0 - 301.0_f32.sqrt() / 90.0;
    assert!(
        hits.len() == 1 && (hits[0].depth - want).abs() < 1e-5,
        "{hits:?}"
    );
}

#[test]
fn each_tier_s_hits_lie_between_the_camera_s_order_and_its_ui_s() {
    // One tier, a quarter above the camera's order; of two, a third and a sixth above, the
    // first higher, each below the UI's half.
    assert_eq!(tier_order(0, 0, 1), 0.25);
    assert_eq!(tier_order(0, 0, 2), 1.0 / 3.0);
    assert_eq!(tier_order(2, 1, 2), 2.0 + 0.5 / 3.0);
}

#[test]
fn the_hover_takes_the_unit_on_top_and_a_ui_node_over_it_hides_it() {
    // A camera at (0, 1, 10) facing −z on a window 800 × 600, the cursor at its center: its
    // ray runs level along −z, through a post at the origin and one behind it at z = −5.
    let mut app = test_app::headless();
    test_app::add_match(&mut app);
    app.add_plugins(UnitPickingPlugin);
    let world = app.world_mut();
    let window = world
        .spawn((
            Window {
                resolution: WindowResolution::new(800, 600),
                ..Window::default()
            },
            PrimaryWindow,
        ))
        .id();
    world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.0, 10.0)));
    let mut ids = IdAllocator::default();
    let [near, behind] = [0.0, -5.0].map(|z| {
        let id = ids.allocate();
        let unit = world.spawn((id, Team::new(0))).id();
        let root =
            (world.spawn((DrawingOf(unit), Transform::from_xyz(0.0, 0.0, z), post(0.5)))).id();
        (id, root)
    });
    app.update();
    app.world_mut()
        .write_message(WindowEvent::CursorMoved(CursorMoved {
            window,
            position: Vec2::new(400.0, 300.0),
            delta: None,
        }));
    app.update();
    let hovered = |app: &App| {
        let map = app.world().resource::<HoverMap>();
        map.get(&PointerId::Mouse)
            .map(|hits| hits.keys().copied().collect::<Vec<_>>())
    };
    // The post in front blocks the one behind; a query that skips it takes the one behind.
    assert_eq!(hovered(&app), Some(vec![near.1]));
    let under = |app: &mut App, except: Vec<StableId>| {
        (app.world_mut())
            .run_system_once(move |pointer: Pointer<'_, '_>| {
                pointer.unit_under(&except).map(|unit| unit.id)
            })
            .unwrap()
    };
    assert_eq!(under(&mut app, vec![]), Some(near.0));
    assert_eq!(under(&mut app, vec![near.0]), Some(behind.0));
    // A node of the UI over the whole window takes the hover from both.
    let node = (app.world_mut())
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..Node::default()
        })
        .id();
    app.update();
    app.update();
    assert_eq!(hovered(&app), Some(vec![node]));
}
