use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::{Camera3d, PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::message::MessageReader;
use bevy::ecs::query::{Changed, With};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::{MouseButton, MouseScrollUnit, MouseWheel};
use bevy::math::Vec2;
use bevy::time::Time;
use bevy::transform::components::Transform;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window};

use crate::input_map::{Act, InputMap};
use crate::view::ViewSystems;
use crate::view::camera_rig::{CameraRig, CameraTurn, ScreenPoint, Zoom};
use crate::view::client_data::ClientData;
use crate::view::float_num::FloatNum;
use crate::view::ground_heights::GroundHeights;
use crate::view::scene_lights::EXPOSURE;

/// The player's camera, as its package's camera file and the key map move it: the scroll chords
/// and the window's edges scroll it, the wheel and the zoom chords zoom it, the turn chords turn
/// it, a drag of the middle button pans it, keeping the ground it grabbed under the cursor, as an
/// editor's and an RTS's do, and with the orbit's modifiers orbits it, turning and tilting; a
/// click of the middle button or the reset chord sets it back. It starts over the middle of the
/// map's heightmap, or the origin, and stays over the heightmap. The window keeps the cursor
/// within it while it has the focus, so the cursor reaches the edges that scroll, and lets it go
/// when it loses the focus.
#[derive(Debug)]
pub(crate) struct PlayerCameraPlugin;

/// The primary window and its cursor, in a frame the window changed.
type FocusChanged<'w, 's> = Query<
    'w,
    's,
    (&'static Window, &'static mut CursorOptions),
    (With<PrimaryWindow>, Changed<Window>),
>;

/// The pixels of a trackpad's scroll that count as a notch of a wheel.
const PIXELS_PER_NOTCH: f32 = 50.0;

impl Plugin for PlayerCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::spawn);
        app.add_systems(
            Update,
            (Self::steer, Self::place.run_if(Self::is_moved))
                .chain()
                .before(ViewSystems),
        );
        app.add_systems(Update, Self::confine);
    }
}

impl PlayerCameraPlugin {
    fn spawn(data: Res<'_, ClientData>, input: Res<'_, InputMap>, mut commands: Commands<'_, '_>) {
        let file = data.camera.unwrap_or(CameraRig::DEFAULT);
        let grid = data.heights.as_ref().map(GroundHeights::grid);
        let bounds = grid.map(|grid| {
            let [x, z] = grid.origin().map(FloatNum::float);
            let [far_x, far_z] = grid.far_corner().map(FloatNum::float);
            [
                Vec2::new(x.min(far_x), z.min(far_z)),
                Vec2::new(x.max(far_x), z.max(far_z)),
            ]
        });
        let pivot = bounds.map_or(Vec2::ZERO, |[least, most]| (least + most) / 2.0);
        let rig = CameraRig::new(file, pivot, bounds).with_press(input.press());
        // No tonemapping: the default one needs lookup tables the client does not build with. With
        // none, Bevy dithers nothing, so the dither its camera requires goes too.
        commands.spawn((
            Camera3d::default(),
            Tonemapping::None,
            DebandDither::Disabled,
            EXPOSURE,
            rig.transform(0.0),
        ));
        commands.insert_resource(rig);
    }

    /// Moves the camera by this frame's keys, wheel and cursor.
    fn steer(
        (keys, input): (Res<'_, ButtonInput<KeyCode>>, Res<'_, InputMap>),
        buttons: Res<'_, ButtonInput<MouseButton>>,
        mut wheel: MessageReader<'_, '_, MouseWheel>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        time: Res<'_, Time>,
        data: Res<'_, ClientData>,
        mut rig: ResMut<'_, CameraRig>,
    ) {
        let seconds = time.delta_secs();
        let now = time.elapsed_secs_f64();
        let held = |act| input.is_held(act, &keys);
        let axis = |minus: bool, plus: bool| f32::from(i8::from(plus) - i8::from(minus));
        let cursor = window.cursor_position();
        let width = rig.edge();
        let edge = |at: Option<f32>, size: f32| {
            at.map_or([false; 2], |at| [at < width, at >= size - width])
        };
        let [left, right] = edge(cursor.map(|cursor| cursor.x), window.width());
        let [top, bottom] = edge(cursor.map(|cursor| cursor.y), window.height());
        let across = axis(
            held(Act::ScrollLeft) || left,
            held(Act::ScrollRight) || right,
        );
        let along = axis(held(Act::ScrollDown) || bottom, held(Act::ScrollUp) || top);
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
        match (held(Act::ZoomIn), held(Act::ZoomOut)) {
            (true, false) => rig.zoom_key(Zoom::In, seconds),
            (false, true) => rig.zoom_key(Zoom::Out, seconds),
            _ => {}
        }
        match (held(Act::TurnLeft), held(Act::TurnRight)) {
            (true, false) => rig.turn_key(CameraTurn::Clockwise, seconds),
            (false, true) => rig.turn_key(CameraTurn::Counterclockwise, seconds),
            _ => {}
        }
        if input.pressed(Act::ResetCamera, &keys) {
            rig.reset();
        }
        if let Some(cursor) = cursor {
            if buttons.just_pressed(MouseButton::Middle) {
                if input.orbits(&keys) {
                    rig.press_orbit(cursor, now);
                } else {
                    let pivot = rig.pivot();
                    let ground = data
                        .heights
                        .as_ref()
                        .map_or(0.0, |heights| heights.at(pivot.x, pivot.y));
                    let screen = Vec2::new(window.width(), window.height());
                    rig.press_pan(ScreenPoint { cursor, screen }, ground, now);
                }
            }
            // A rig changes only as it moves, so the camera is placed only then.
            if rig.is_dragging() {
                rig.drag_to(cursor);
            }
        }
        if buttons.just_released(MouseButton::Middle) && rig.is_dragging() {
            rig.release(now);
        }
    }

    /// Whether the camera's place or field of view may differ from the rig's: the rig moved, or
    /// the window changed its shape.
    fn is_moved(
        rig: Res<'_, CameraRig>,
        resized: Query<'_, '_, (), (Changed<Window>, With<PrimaryWindow>)>,
    ) -> bool {
        rig.is_changed() || !resized.is_empty()
    }

    /// Keeps the cursor within the window while the window has the focus, and lets it go when
    /// the window loses it, as a window's focus changes.
    fn confine(mut windows: FocusChanged<'_, '_>) {
        for (window, mut cursor) in &mut windows {
            let grab = if window.focused {
                CursorGrabMode::Confined
            } else {
                CursorGrabMode::None
            };
            if cursor.grab_mode != grab {
                cursor.grab_mode = grab;
            }
        }
    }

    /// Puts the camera where the rig says, its pivot on the ground, and its field of view on the
    /// window's shape.
    fn place(
        rig: Res<'_, CameraRig>,
        data: Res<'_, ClientData>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        camera: Single<'_, '_, (&mut Transform, &mut Projection), With<Camera3d>>,
    ) {
        let pivot = rig.pivot();
        let ground = data
            .heights
            .as_ref()
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

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::ecs::world::World;

    use super::*;

    #[test]
    fn the_cursor_stays_in_the_window_while_it_has_the_focus() {
        let mut world = World::new();
        let window = world
            .spawn((
                Window {
                    focused: true,
                    ..Window::default()
                },
                CursorOptions::default(),
                PrimaryWindow,
            ))
            .id();
        let grab = |world: &mut World| {
            world.run_system_once(PlayerCameraPlugin::confine).unwrap();
            world.get::<CursorOptions>(window).unwrap().grab_mode
        };
        assert_eq!(grab(&mut world), CursorGrabMode::Confined);
        world.get_mut::<Window>(window).unwrap().focused = false;
        assert_eq!(grab(&mut world), CursorGrabMode::None);
        world.get_mut::<Window>(window).unwrap().focused = true;
        assert_eq!(grab(&mut world), CursorGrabMode::Confined);
    }
}
