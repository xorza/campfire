use super::*;

/// Zero Hour's shipped camera: 37.5° down, 120 m to 310 m, 700 m/s across, 875 m/s along.
const SHIPPED: CameraFile = CameraFile {
    pitch: 37.5,
    yaw: 0.0,
    field_of_view: 50.0,
    height: CameraHeight {
        least: 120.0,
        most: 310.0,
    },
    scroll: Vec2::new(700.0, 875.0),
    turn: 3.0,
    drag_turn: 0.01,
    zoom_notch: 10.0,
    zoom_key: 300.0,
    edge: 3.0,
};
/// A click of up to 167 ms, the shipped `Mouse.ini`'s.
const PRESS: Press = Press {
    drag: 4.0,
    click: 0.167,
};

fn near(a: Vec3, b: Vec3) -> bool {
    // The tangent and the look rotation are the platform's floats.
    (a - b).length() < 1e-3
}

#[test]
fn the_camera_starts_at_its_most_height_back_along_its_pitch_looking_at_the_pivot() {
    let rig = CameraRig::new(SHIPPED, Vec2::new(100.0, -50.0), None);
    // 310 m up over ground at 20 m, back 310 / tan 37.5° = 310 / 0.7673270 = 404.0001 m
    // south, the engine's +z.
    let eye = rig.transform(20.0);
    assert!(near(
        eye.translation,
        Vec3::new(100.0, 330.0, -50.0 + 404.0001)
    ));
    assert!(near(
        eye.forward().into(),
        Vec3::new(0.0, -0.608_761_4, -0.793_353_3)
    ));
    // The forward is (0, −sin 37.5°, −cos 37.5°): looking north and down.
    // A quarter turn counter-clockwise from above, looking west: back is now east, +x.
    let mut turned = rig.clone();
    turned.turn_key(CameraTurn::Counterclockwise, PI / 2.0 / 3.0);
    let eye = turned.transform(20.0);
    assert!(near(
        eye.translation,
        Vec3::new(100.0 + 404.0001, 330.0, -50.0)
    ));
    // Its report holds the rig's own state, which puts it back exactly, and the eye it placed.
    let report = turned.report(&eye);
    assert_eq!(report.pivot, [100.0, -50.0]);
    assert_eq!(
        [report.yaw, report.pitch, report.height],
        [turned.yaw, 37.5_f32.to_radians(), 310.0]
    );
    assert_eq!(report.place, eye.translation.to_array());
    assert_eq!(report.forward, eye.forward().to_array());
    // The default's view of old: 24 m up and 18 m back.
    let old = CameraRig::new(CameraRig::DEFAULT, Vec2::ZERO, None).transform(0.0);
    assert!(near(old.translation, Vec3::new(0.0, 24.0, 18.0)));
    // A horizontal 50° on a 16:9 screen is 2 · atan(tan 25° · 9 / 16) = 2 · atan(0.2622982)
    // = 2 · 14.69748° = 29.39496°.
    let fov = rig.vertical_fov(16.0 / 9.0).to_degrees();
    assert!((fov - 29.394_96).abs() < 1e-3, "{fov}");
}

#[test]
fn zoom_stops_at_each_limit_and_a_click_sets_the_view_back() {
    let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, None).with_press(PRESS);
    // Out from the most stays at it; 20 notches in, 200 m, stop at the least, 120 m.
    rig.wheel(-1.0);
    assert_eq!(rig.height, 310.0);
    rig.wheel(20.0);
    assert_eq!(rig.height, 120.0);
    // A key zooms 300 m a second: half a second out from 120 is 270.
    rig.zoom_key(Zoom::Out, 0.5);
    assert_eq!(rig.height, 270.0);
    // An orbit of 50 pixels right turns 0.01 rad a pixel, 0.5 rad counter-clockwise, and of
    // 30 down tilts 0.3 rad more, from 37.5°; a still click sets it all back.
    let start = 37.5_f32.to_radians();
    rig.press_orbit(Vec2::new(400.0, 300.0), 10.0);
    rig.drag_to(Vec2::new(450.0, 330.0));
    assert_eq!((rig.yaw, rig.pitch), (0.5, start + 30.0 * 0.01));
    rig.release(10.1);
    assert_eq!((rig.yaw, rig.height), (0.5, 270.0));
    // Tilts stop at 10° and at 89°.
    rig.press_orbit(Vec2::new(450.0, 300.0), 10.5);
    rig.drag_to(Vec2::new(450.0, 1000.0));
    assert_eq!(rig.pitch, PITCH_MOST);
    rig.drag_to(Vec2::new(450.0, -1000.0));
    assert_eq!(rig.pitch, PITCH_LEAST);
    rig.release(11.0);
    rig.press_orbit(Vec2::new(450.0, 300.0), 11.0);
    rig.drag_to(Vec2::new(452.0, 301.0));
    rig.release(11.1);
    assert_eq!((rig.pitch, rig.yaw, rig.height), (start, 0.0, 310.0));
    // A still press longer than 167 ms is no click.
    rig.wheel(2.0);
    rig.press_orbit(Vec2::ZERO, 12.0);
    rig.release(12.2);
    assert_eq!(rig.height, 290.0);
    // A press of a click of 0.5 s takes that press as a click.
    let slow = Press {
        click: 0.5,
        ..PRESS
    };
    let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, None).with_press(slow);
    rig.wheel(2.0);
    rig.press_orbit(Vec2::ZERO, 12.0);
    rig.release(12.2);
    assert_eq!(rig.height, 310.0);
}

#[test]
fn a_pan_keeps_the_ground_it_grabbed_under_the_cursor() {
    let screen = Vec2::new(1600.0, 900.0);
    let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, None);
    rig.turn_to(0.3);
    // The camera looks at its pivot through the screen's center, the ground 20 m up.
    let center = screen / 2.0;
    let eye = rig.transform(20.0);
    let under = rig
        .ground_under(
            &eye,
            ScreenPoint {
                cursor: center,
                screen,
            },
            20.0,
        )
        .unwrap();
    assert!(under.length() < 1e-3, "{under}");
    // A drag from a point to another keeps the point it grabbed under the cursor: on the
    // camera that drag ends with, the ground under the cursor is the grabbed point. A click
    // ends nothing of a pan's place: the drag moved.
    let from = Vec2::new(500.0, 600.0);
    let grabbed = rig
        .ground_under(
            &eye,
            ScreenPoint {
                cursor: from,
                screen,
            },
            20.0,
        )
        .unwrap();
    rig.press_pan(
        ScreenPoint {
            cursor: from,
            screen,
        },
        20.0,
        1.0,
    );
    let to = Vec2::new(1100.0, 300.0);
    rig.drag_to(to);
    let moved = rig.pivot();
    assert!(moved.length() > 100.0, "{moved}");
    let after = rig
        .ground_under(
            &rig.transform(20.0),
            ScreenPoint { cursor: to, screen },
            20.0,
        )
        .unwrap();
    assert!((after - grabbed).length() < 1e-2, "{after} {grabbed}");
    rig.release(1.1);
    assert_eq!(rig.pivot(), moved);
    // Tilted to 10°, the screen's top looks 10° − 14.7° above the horizon: a pan grabbed
    // there grabs no ground, and moves nothing.
    rig.pitch = PITCH_LEAST;
    rig.press_pan(
        ScreenPoint {
            cursor: Vec2::new(800.0, 0.0),
            screen,
        },
        20.0,
        2.0,
    );
    rig.drag_to(Vec2::new(800.0, 500.0));
    assert_eq!(rig.pivot(), moved);
    // A screen of no pixel, as a minimized window's, grabs nothing.
    let nowhere = ScreenPoint {
        cursor: Vec2::ZERO,
        screen: Vec2::ZERO,
    };
    rig.press_pan(nowhere, 20.0, 3.0);
    rig.drag_to(Vec2::new(10.0, 10.0));
    assert_eq!(rig.pivot(), moved);
}

#[test]
fn scrolling_moves_the_pivot_across_and_along_the_screen_within_the_map() {
    let bounds = Some([Vec2::new(-1000.0, -1000.0), Vec2::new(1000.0, 1000.0)]);
    let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, bounds);
    // A tenth of a second right: 70 m east; along: 87.5 m north, the engine's −z.
    rig.scroll(1.0, 0.0, 0.1);
    rig.scroll(0.0, 1.0, 0.1);
    assert_eq!(rig.pivot(), Vec2::new(70.0, -87.5));
    // Turned a quarter counter-clockwise, looking west, the screen's right is north, −z.
    rig.turn_to(PI / 2.0);
    rig.scroll(1.0, 0.0, 0.1);
    assert!((rig.pivot() - Vec2::new(70.0, -157.5)).length() < 1e-3);
    // Two seconds north stop at the map's edge.
    rig.turn_to(0.0);
    rig.scroll(0.0, 1.0, 2.0);
    assert_eq!(rig.pivot().y, -1000.0);
}
