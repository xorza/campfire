use std::f32::consts::PI;

use bevy::ecs::resource::Resource;
use bevy::math::{Quat, Vec2, Vec3};
use bevy::transform::components::Transform;
use campfire_client_files::{CameraFile, CameraHeight};

/// The player's camera: it looks down at a pitch at a point of the ground, its pivot, from a
/// height above it, turned about it by a yaw. Keys and the screen's edges scroll the pivot across
/// and along the screen, the wheel and keys zoom the height between its least and its most, and
/// keys turn it. A drag pans it, keeping the ground it grabbed under the cursor, or orbits it,
/// turning it and tilting it; a click of the drag's button, short and still, sets the pitch, the
/// yaw and the height back to their start, its file's.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct CameraRig {
    file: CameraFile,
    /// On the ground plane, `[x, z]`.
    pivot: Vec2,
    /// Radians down from level, between `PITCH_LEAST` and `PITCH_MOST`.
    pitch: f32,
    /// Radians, counter-clockwise seen from above, from looking north, the engine's `−z`.
    yaw: f32,
    height: f32,
    /// The least and the most corner the pivot stays within, when the map has them.
    bounds: Option<[Vec2; 2]>,
    drag: Option<Drag>,
}

/// A drag of the camera: where the cursor began, in pixels, when it began, in seconds, whether it
/// went far enough to be no click, and what it does.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    from: Vec2,
    since: f64,
    moved: bool,
    kind: DragKind,
}

/// What a drag does: a pan, of the camera as it stood as the drag began, a screen of `screen`
/// pixels, the ground's height at its pivot, and the point of that ground the cursor grabbed,
/// none when the cursor was above the horizon; or an orbit, from the yaw and the pitch it began
/// at.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DragKind {
    Pan {
        pivot: Vec2,
        eye: Transform,
        screen: Vec2,
        ground: f32,
        grabbed: Option<Vec2>,
    },
    Orbit {
        yaw: f32,
        pitch: f32,
    },
}

/// The least and the most pitch an orbit tilts to: below 10° the camera skims the map and sees
/// far past it, and at 90° it looks straight down, where a look has no up.
const PITCH_LEAST: f32 = 10.0 * PI / 180.0;
const PITCH_MOST: f32 = 89.0 * PI / 180.0;

/// A press shorter than this, in seconds, is a click: the game's 167 ms.
const CLICK: f64 = 0.167;
/// A drag past this many pixels on an axis is no click.
const STILL: f32 = 5.0;

impl CameraRig {
    /// The camera a package with no camera file has: where the client's camera of old stood,
    /// 24 m above the point in view and 18 m south of it, zooming between 12 m and 24 m.
    pub(crate) const DEFAULT: CameraFile = CameraFile {
        pitch: 53.130_1,
        yaw: 0.0,
        field_of_view: 60.0,
        height: CameraHeight {
            least: 12.0,
            most: 24.0,
        },
        scroll: Vec2::splat(20.0),
        turn: 3.0,
        drag_turn: 0.01,
        zoom_notch: 1.0,
        zoom_key: 12.0,
    };

    /// The camera of `file`, looking at `pivot` at its start's yaw and its most height, within
    /// `bounds`.
    pub(crate) fn new(file: CameraFile, pivot: Vec2, bounds: Option<[Vec2; 2]>) -> CameraRig {
        let mut rig = CameraRig {
            file,
            pivot,
            pitch: 0.0,
            yaw: 0.0,
            height: 0.0,
            bounds,
            drag: None,
        };
        rig.reset();
        rig.pivot = rig.bounded(pivot);
        rig
    }

    pub(crate) const fn pivot(&self) -> Vec2 {
        self.pivot
    }

    /// Where the camera stands and how it looks, its pivot at `ground`'s height: back from the
    /// pivot by its height over the tangent of its pitch, turned by its yaw, looking at it.
    pub(crate) fn transform(&self, ground: f32) -> Transform {
        let target = Vec3::new(self.pivot.x, ground, self.pivot.y);
        let back = self.height / self.pitch.tan();
        let offset = Quat::from_rotation_y(self.yaw) * Vec3::new(0.0, self.height, back);
        Transform::from_translation(target + offset).looking_at(target, Vec3::Y)
    }

    /// Its vertical field of view, in radians, on a screen `aspect` times as wide as tall.
    pub(crate) fn vertical_fov(&self, aspect: f32) -> f32 {
        let half = (self.file.field_of_view.to_radians() / 2.0).tan();
        2.0 * (half / aspect).atan()
    }

    /// Scrolls the pivot for `seconds` by `across`, to the screen's right, and `along`, to its
    /// top, each from −1 to 1 of its speed on that axis.
    pub(crate) fn scroll(&mut self, across: f32, along: f32, seconds: f32) {
        let turn = Quat::from_rotation_y(self.yaw);
        let right = turn * Vec3::X;
        let forward = turn * Vec3::NEG_Z;
        let step = Vec2::new(across, along) * self.file.scroll * seconds;
        let moved = right * step.x + forward * step.y;
        self.pivot = self.bounded(self.pivot + Vec2::new(moved.x, moved.z));
    }

    /// Zooms by `notches` of the wheel, up and away from the player zooming in.
    pub(crate) fn wheel(&mut self, notches: f32) {
        self.zoom(-notches * self.file.zoom_notch);
    }

    /// Zooms for `seconds` of a key, `out` or in.
    pub(crate) fn zoom_key(&mut self, out: bool, seconds: f32) {
        let by = self.file.zoom_key * seconds;
        self.zoom(if out { by } else { -by });
    }

    /// Turns for `seconds` of a key, `clockwise` seen from above or counter-clockwise: the
    /// game's left key turns the camera clockwise, so the world turns left on the screen.
    pub(crate) fn turn_key(&mut self, clockwise: bool, seconds: f32) {
        let by = self.file.turn * seconds;
        self.turn_to(self.yaw + if clockwise { -by } else { by });
    }

    /// Begins a pan at `cursor`, `now` seconds, on a screen of `screen` pixels, the ground at
    /// the pivot `ground` high: it grabs the point of that ground under the cursor.
    pub(crate) fn press_pan(&mut self, cursor: Vec2, screen: Vec2, ground: f32, now: f64) {
        let eye = self.transform(ground);
        let grabbed = self.ground_under(&eye, cursor, screen, ground);
        self.drag = Some(Drag {
            from: cursor,
            since: now,
            moved: false,
            kind: DragKind::Pan {
                pivot: self.pivot,
                eye,
                screen,
                ground,
                grabbed,
            },
        });
    }

    /// Begins an orbit at `cursor`, `now` seconds.
    pub(crate) const fn press_orbit(&mut self, cursor: Vec2, now: f64) {
        self.drag = Some(Drag {
            from: cursor,
            since: now,
            moved: false,
            kind: DragKind::Orbit {
                yaw: self.yaw,
                pitch: self.pitch,
            },
        });
    }

    /// Moves by the drag to `cursor`. A pan moves the pivot by what the ground under the cursor
    /// moved on its camera as it began, so the point it grabbed stays under the cursor; the ground
    /// past the horizon moves it none. An orbit turns its file's radians a pixel the cursor goes
    /// right, counter-clockwise seen from above, as the game's drag does, and tilts as many a
    /// pixel it goes down, the scene following the cursor as Blender's turntable does.
    pub(crate) fn drag_to(&mut self, cursor: Vec2) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let moved = cursor - drag.from;
        drag.moved |= moved.x.abs() > STILL || moved.y.abs() > STILL;
        match drag.kind {
            DragKind::Pan {
                pivot,
                eye,
                screen,
                ground,
                grabbed,
            } => {
                let under = self.ground_under(&eye, cursor, screen, ground);
                if let (Some(grabbed), Some(under)) = (grabbed, under) {
                    self.pivot = self.bounded(pivot + grabbed - under);
                }
            }
            DragKind::Orbit { yaw, pitch } => {
                let rate = self.file.drag_turn;
                self.turn_to(yaw + moved.x * rate);
                self.pitch = (pitch + moved.y * rate).clamp(PITCH_LEAST, PITCH_MOST);
            }
        }
    }

    /// The point `[x, z]` of the ground plane `ground` high under `cursor`, on a screen of
    /// `screen` pixels, seen by `eye`; none past the horizon, and none on a screen of no pixel, as
    /// a minimized window's, where no ray reaches the ground.
    fn ground_under(
        &self,
        eye: &Transform,
        cursor: Vec2,
        screen: Vec2,
        ground: f32,
    ) -> Option<Vec2> {
        if screen.x < 1.0 || screen.y < 1.0 {
            return None;
        }
        let aspect = screen.x / screen.y;
        let tan = (self.vertical_fov(aspect) / 2.0).tan();
        let ndc = Vec2::new(
            2.0 * cursor.x / screen.x - 1.0,
            1.0 - 2.0 * cursor.y / screen.y,
        );
        let ray = eye.rotation * Vec3::new(ndc.x * tan * aspect, ndc.y * tan, -1.0);
        if ray.y >= 0.0 {
            return None;
        }
        let at = eye.translation + ray * ((ground - eye.translation.y) / ray.y);
        Some(Vec2::new(at.x, at.z))
    }

    /// Ends the drag, `now` seconds: a click sets the pitch, the yaw and the height back to their
    /// start.
    pub(crate) fn release(&mut self, now: f64) {
        if let Some(drag) = self.drag.take()
            && !drag.moved
            && now - drag.since < CLICK
        {
            self.reset();
        }
    }

    /// Sets the pitch, the yaw and the height back to their start.
    pub(crate) const fn reset(&mut self) {
        self.pitch = self.file.pitch.to_radians();
        self.yaw = self.file.yaw.to_radians();
        self.height = self.file.height.most;
    }

    fn zoom(&mut self, by: f32) {
        let CameraHeight { least, most } = self.file.height;
        self.height = (self.height + by).clamp(least, most);
    }

    /// Takes the yaw `yaw`, from −π to π.
    fn turn_to(&mut self, yaw: f32) {
        self.yaw = (yaw + PI).rem_euclid(2.0 * PI) - PI;
    }

    fn bounded(&self, pivot: Vec2) -> Vec2 {
        match self.bounds {
            Some([least, most]) => pivot.clamp(least, most),
            None => pivot,
        }
    }
}

#[cfg(test)]
mod tests {
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
        turned.turn_key(false, PI / 2.0 / 3.0);
        let eye = turned.transform(20.0);
        assert!(near(
            eye.translation,
            Vec3::new(100.0 + 404.0001, 330.0, -50.0)
        ));
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
        let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, None);
        // Out from the most stays at it; 20 notches in, 200 m, stop at the least, 120 m.
        rig.wheel(-1.0);
        assert_eq!(rig.height, 310.0);
        rig.wheel(20.0);
        assert_eq!(rig.height, 120.0);
        // A key zooms 300 m a second: half a second out from 120 is 270.
        rig.zoom_key(true, 0.5);
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
    }

    #[test]
    fn a_pan_keeps_the_ground_it_grabbed_under_the_cursor() {
        let screen = Vec2::new(1600.0, 900.0);
        let mut rig = CameraRig::new(SHIPPED, Vec2::ZERO, None);
        rig.turn_to(0.3);
        // The camera looks at its pivot through the screen's center, the ground 20 m up.
        let center = screen / 2.0;
        let eye = rig.transform(20.0);
        let under = rig.ground_under(&eye, center, screen, 20.0).unwrap();
        assert!(under.length() < 1e-3, "{under}");
        // A drag from a point to another keeps the point it grabbed under the cursor: on the
        // camera that drag ends with, the ground under the cursor is the grabbed point. A click
        // ends nothing of a pan's place: the drag moved.
        let from = Vec2::new(500.0, 600.0);
        let grabbed = rig.ground_under(&eye, from, screen, 20.0).unwrap();
        rig.press_pan(from, screen, 20.0, 1.0);
        let to = Vec2::new(1100.0, 300.0);
        rig.drag_to(to);
        let moved = rig.pivot();
        assert!(moved.length() > 100.0, "{moved}");
        let after = rig
            .ground_under(&rig.transform(20.0), to, screen, 20.0)
            .unwrap();
        assert!((after - grabbed).length() < 1e-2, "{after} {grabbed}");
        rig.release(1.1);
        assert_eq!(rig.pivot(), moved);
        // Tilted to 10°, the screen's top looks 10° − 14.7° above the horizon: a pan grabbed
        // there grabs no ground, and moves nothing.
        rig.pitch = PITCH_LEAST;
        rig.press_pan(Vec2::new(800.0, 0.0), screen, 20.0, 2.0);
        rig.drag_to(Vec2::new(800.0, 500.0));
        assert_eq!(rig.pivot(), moved);
        // A screen of no pixel, as a minimized window's, grabs nothing.
        rig.press_pan(Vec2::ZERO, Vec2::ZERO, 20.0, 3.0);
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
}
