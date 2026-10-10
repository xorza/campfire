use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::{Camera3d, PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::{MouseButton, MouseScrollUnit, MouseWheel};
use bevy::math::Vec2;
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy::window::{PrimaryWindow, Window};

use crate::view::ViewSystems;
use crate::view::camera_rig::CameraRig;
use crate::view::client_data::ClientData;
use crate::view::float_num::FloatNum;

/// The player's camera, as its package's camera file and the game's controls move it: the arrow
/// keys and the window's edges scroll it, the wheel and the keypad's 8 and 2 zoom it, a drag of
/// the middle button and the keypad's 4 and 6 turn it, and a click of the middle button or the
/// keypad's 5 sets it back. It starts over the middle of the map's heightmap, or the origin, and
/// stays over the heightmap.
#[derive(Debug)]
pub(crate) struct PlayerCamera;

/// The pixels from the window's edge within which the cursor scrolls, as the game's 3.
const EDGE: f32 = 3.0;
/// The pixels of a trackpad's scroll that count as a notch of a wheel.
const PIXELS_PER_NOTCH: f32 = 50.0;

impl Plugin for PlayerCamera {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, PlayerCamera::spawn);
        app.add_systems(
            Update,
            (PlayerCamera::steer, PlayerCamera::place)
                .chain()
                .before(ViewSystems),
        );
    }
}

impl PlayerCamera {
    fn spawn(data: Option<Res<'_, ClientData>>, mut commands: Commands<'_, '_>) {
        let data = data.as_deref();
        let file = data
            .and_then(|data| data.camera)
            .unwrap_or(CameraRig::DEFAULT);
        let grid = data.and_then(|data| data.grid.as_ref());
        let bounds = grid.map(|grid| {
            let [x, z] = grid.origin().map(FloatNum::float);
            let [far_x, far_z] = grid.far_corner().map(FloatNum::float);
            [
                Vec2::new(x.min(far_x), z.min(far_z)),
                Vec2::new(x.max(far_x), z.max(far_z)),
            ]
        });
        let pivot = bounds.map_or(Vec2::ZERO, |[least, most]| (least + most) / 2.0);
        let rig = CameraRig::new(file, pivot, bounds);
        // No tonemapping: the default one needs lookup tables the client does not build with.
        commands.spawn((Camera3d::default(), Tonemapping::None, rig.transform(0.0)));
        commands.insert_resource(rig);
    }

    /// Moves the camera by this frame's keys, wheel and cursor.
    fn steer(
        keys: Res<'_, ButtonInput<KeyCode>>,
        buttons: Res<'_, ButtonInput<MouseButton>>,
        mut wheel: MessageReader<'_, '_, MouseWheel>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        time: Res<'_, Time>,
        mut rig: ResMut<'_, CameraRig>,
    ) {
        let seconds = time.delta_secs();
        let now = time.elapsed_secs_f64();
        let held = |key| keys.pressed(key);
        let axis = |minus: bool, plus: bool| f32::from(i8::from(plus) - i8::from(minus));
        let cursor = window.cursor_position();
        let edge =
            |at: Option<f32>, size: f32| at.map_or([false; 2], |at| [at < EDGE, at >= size - EDGE]);
        let [left, right] = edge(cursor.map(|cursor| cursor.x), window.width());
        let [top, bottom] = edge(cursor.map(|cursor| cursor.y), window.height());
        let across = axis(
            held(KeyCode::ArrowLeft) || left,
            held(KeyCode::ArrowRight) || right,
        );
        let along = axis(
            held(KeyCode::ArrowDown) || bottom,
            held(KeyCode::ArrowUp) || top,
        );
        if across != 0.0 || along != 0.0 {
            rig.scroll(across, along, seconds);
        }
        let notches: f32 = wheel
            .read()
            .map(|turn| match turn.unit {
                MouseScrollUnit::Line => turn.y,
                MouseScrollUnit::Pixel => turn.y / PIXELS_PER_NOTCH,
            })
            .sum();
        if notches != 0.0 {
            rig.wheel(notches);
        }
        match (held(KeyCode::Numpad8), held(KeyCode::Numpad2)) {
            (true, false) => rig.zoom_key(false, seconds),
            (false, true) => rig.zoom_key(true, seconds),
            _ => {}
        }
        match (held(KeyCode::Numpad4), held(KeyCode::Numpad6)) {
            (true, false) => rig.turn_key(true, seconds),
            (false, true) => rig.turn_key(false, seconds),
            _ => {}
        }
        if keys.just_pressed(KeyCode::Numpad5) {
            rig.reset();
        }
        if let Some(cursor) = cursor {
            if buttons.just_pressed(MouseButton::Middle) {
                rig.press(cursor, now);
            }
            rig.drag_to(cursor);
        }
        if buttons.just_released(MouseButton::Middle) {
            rig.release(now);
        }
    }

    /// Puts the camera where the rig says, its pivot on the ground, and its field of view on the
    /// window's shape.
    fn place(
        rig: Res<'_, CameraRig>,
        data: Option<Res<'_, ClientData>>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        camera: Single<'_, '_, (&mut Transform, &mut Projection), With<Camera3d>>,
    ) {
        let pivot = rig.pivot();
        let ground = data
            .as_deref()
            .and_then(|data| data.heights.as_ref())
            .map_or(0.0, |heights| heights.at(pivot.x, pivot.y));
        let (mut transform, mut projection) = camera.into_inner();
        let placed = rig.transform(ground);
        if *transform != placed {
            *transform = placed;
        }
        let aspect = window.width() / window.height().max(1.0);
        let fov = rig.vertical_fov(aspect);
        if let Projection::Perspective(PerspectiveProjection { fov: shown, .. }) = &*projection
            && shown.to_bits() == fov.to_bits()
        {
            return;
        }
        *projection = Projection::Perspective(PerspectiveProjection {
            fov,
            ..PerspectiveProjection::default()
        });
    }
}
