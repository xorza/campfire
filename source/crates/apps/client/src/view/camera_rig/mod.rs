use std::f32::consts::PI;

use bevy::ecs::resource::Resource;
use bevy::math::{Quat, Vec2, Vec3};
use bevy::transform::components::Transform;
use campfire_client_files::{CameraFile, CameraHeight, Press};
use campfire_host::ReportCamera;

/// The player's camera: it looks down at a pitch at a point of the ground, its pivot, from a
/// height above it, turned about it by a yaw. Keys and the screen's edges scroll the pivot across
/// and along the screen, the wheel and keys zoom the height between its least and its most, and
/// keys turn it. A drag pans it, keeping the ground it grabbed under the cursor, or orbits it,
/// turning it and tilting it; a click of the drag's button, short and still, sets the pitch, the
/// yaw and the height back to their start, its file's, as its press's rule says.
#[derive(Resource, Debug, Clone, PartialEq)]
pub(crate) struct CameraRig {
    file: CameraFile,
    press: Press,
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

/// Which way a key zooms the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zoom {
    In,
    Out,
}

/// Which way a key turns the camera, seen from above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CameraTurn {
    Clockwise,
    Counterclockwise,
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

/// A point of the window, the cursor's, in logical pixels from its top left, on a window of
/// `screen` pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenPoint {
    pub(crate) cursor: Vec2,
    pub(crate) screen: Vec2,
}

impl CameraRig {
    /// The camera a package with no camera file has: where the client's camera of old stood,
    /// 24 m above the point in view and 18 m south of it, zooming between 12 m and 24 m, the
    /// cursor scrolling within 3 pixels of the edge, and a click a press of 0.2 s at most.
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
        edge: 3.0,
    };

    /// The camera of `file`, looking at `pivot` at its start's yaw and its most height, within
    /// `bounds`.
    pub(crate) fn new(file: CameraFile, pivot: Vec2, bounds: Option<[Vec2; 2]>) -> Self {
        let mut rig = Self {
            file,
            press: Press::default(),
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

    /// The camera whose presses are a drag and a click by `press`'s rule.
    #[must_use]
    pub(crate) const fn with_press(mut self, press: Press) -> Self {
        self.press = press;
        self
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

    /// The rig as a report holds it, with the place and the forward of `placed`, the camera it
    /// placed (02-engine-core.md, The camera is the rig's state).
    pub(crate) fn report(&self, placed: &Transform) -> ReportCamera {
        ReportCamera {
            pivot: self.pivot.to_array(),
            yaw: self.yaw,
            pitch: self.pitch,
            height: self.height,
            place: placed.translation.to_array(),
            forward: placed.forward().to_array(),
        }
    }

    /// Puts the rig where the report's `camera` stood: its pivot, its yaw, its pitch and its
    /// height, exactly as the report holds them (02-engine-core.md, The camera is the rig's state).
    pub(crate) const fn put(&mut self, camera: &ReportCamera) {
        self.pivot = Vec2::from_array(camera.pivot);
        self.yaw = camera.yaw;
        self.pitch = camera.pitch;
        self.height = camera.height;
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

    /// Zooms `zoom` for `seconds` of a key.
    pub(crate) fn zoom_key(&mut self, zoom: Zoom, seconds: f32) {
        let by = self.file.zoom_key * seconds;
        self.zoom(match zoom {
            Zoom::Out => by,
            Zoom::In => -by,
        });
    }

    /// Turns `turn`, seen from above, for `seconds` of a key: the game's left key turns the
    /// camera clockwise, so the world turns left on the screen.
    pub(crate) fn turn_key(&mut self, turn: CameraTurn, seconds: f32) {
        let by = self.file.turn * seconds;
        self.turn_to(match turn {
            CameraTurn::Clockwise => self.yaw - by,
            CameraTurn::Counterclockwise => self.yaw + by,
        });
    }

    /// Begins a pan at the cursor of `at`, `now` seconds, the ground at the pivot `ground` high:
    /// it grabs the point of that ground under the cursor.
    pub(crate) fn press_pan(&mut self, at: ScreenPoint, ground: f32, now: f64) {
        let ScreenPoint { cursor, screen } = at;
        let eye = self.transform(ground);
        let grabbed = self.ground_under(&eye, at, ground);
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
        drag.moved |= self.press.drags(drag.from, cursor);
        match drag.kind {
            DragKind::Pan {
                pivot,
                eye,
                screen,
                ground,
                grabbed,
            } => {
                let under = self.ground_under(&eye, ScreenPoint { cursor, screen }, ground);
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

    /// The point `[x, z]` of the ground plane `ground` high under the cursor of `at`, seen by
    /// `eye`; none past the horizon, and none on a screen of no pixel, as a minimized window's,
    /// where no ray reaches the ground.
    fn ground_under(&self, eye: &Transform, at: ScreenPoint, ground: f32) -> Option<Vec2> {
        let ScreenPoint { cursor, screen } = at;
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

    /// The pixels from the window's edge within which the cursor scrolls it.
    pub(crate) const fn edge(&self) -> f32 {
        self.file.edge
    }

    /// Whether a pan or an orbit is under way.
    pub(crate) const fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Ends the drag, `now` seconds: a click, a press that did not drag and that its press's
    /// rule takes as one, sets the pitch, the yaw and the height back to their start.
    pub(crate) fn release(&mut self, now: f64) {
        if let Some(drag) = self.drag.take()
            && !drag.moved
            && self.press.clicks(now - drag.since)
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
mod tests;
