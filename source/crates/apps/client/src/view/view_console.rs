use std::time::Duration;

use bevy::app::{App, Plugin, PostUpdate};
use bevy::camera::primitives::Aabb;
use bevy::camera::{Camera, Camera3d};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single, SystemParam};
use bevy::image::Image;
use bevy::math::{URect, UVec2, Vec2, Vec3};
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::{Real, Time};
use bevy::transform::TransformSystems;
use bevy::transform::components::GlobalTransform;
use bevy::window::{PrimaryWindow, Window};
use campfire_codec::{Base64, Png, RgbaImage};
use campfire_common::StableNumber;
use campfire_console::{
    CameraAsk, ConsoleAnswer, ConsoleAsk, ConsoleRefusal, Reply, Request, RequestId, ShotRegion,
    WindowShot,
};
use campfire_host::ReceivedTick;
use campfire_net::ClientConsole;
use campfire_sim::{EntityIndex, StableId, Unpredicted};

use crate::view::camera_rig::CameraRig;
use crate::view::drawing::Drawing;
use crate::view::shot_frame::ShotFrame;

/// The client's view answers its console: a screenshot of its window, a rectangle of it or the
/// box round units on the screen, scaled to Claude's bounds and carried as a PNG in base64, and
/// a read or a move of its camera (console.md, A pixel is the screenshot's). Its screenshots
/// wait a second for their frame.
#[derive(Debug)]
pub(crate) struct ViewConsolePlugin;

/// The screenshots under way, each with when it was asked, by the app's real clock; and the
/// last one's frame, whose pixels a later region names.
#[derive(Resource, Debug, Default)]
struct ViewShots {
    pending: Vec<PendingShot>,
    last: Option<ShotFrame>,
}

/// A screenshot under way: its request, when it was asked, the region it asked, of the frame's
/// pixels, the tick and the camera's pose it drew.
#[derive(Debug, Clone)]
struct PendingShot {
    id: RequestId,
    asked: Duration,
    region: URect,
    tick: Option<u64>,
    camera: campfire_console::CameraPose,
}

/// A frame a screenshot captured.
type Captured<'w, 't> = On<'w, 't, ScreenshotCaptured>;

/// What a captured frame's answer writes: the console's reply, and the screenshots under way.
type ViewWrites<'w> = (ResMut<'w, ClientConsole>, ResMut<'w, ViewShots>);

/// What an answer of the view reads beside its requests.
#[derive(SystemParam)]
struct ViewReads<'w, 's> {
    rig: ResMut<'w, CameraRig>,
    received: Res<'w, ReceivedTick>,
    time: Res<'w, Time<Real>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera3d>>,
    index: Res<'w, EntityIndex>,
    drawings: Query<'w, 's, &'static Drawing, Allow<Unpredicted>>,
    children: Query<'w, 's, &'static Children>,
    bounds: Query<'w, 's, (&'static GlobalTransform, &'static Aabb)>,
    places: Query<'w, 's, &'static GlobalTransform>,
}

/// How long a screenshot waits for its frame: 60 frames at 60 Hz (console.md, A capture waits
/// one second for its frame).
const FRAME_WAIT: Duration = Duration::from_secs(1);

impl Plugin for ViewConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewShots>();
        // After the frame's transforms, so a screenshot's box and its frame show one place.
        app.add_systems(
            PostUpdate,
            Self::answer
                .after(TransformSystems::Propagate)
                .run_if(resource_exists::<ClientConsole>),
        );
    }
}

impl ViewConsolePlugin {
    /// Answers the requests of the view: a camera's at once, a screenshot's once its frame
    /// comes, or as none came in a second.
    fn answer(
        mut console: ResMut<'_, ClientConsole>,
        mut shots: ResMut<'_, ViewShots>,
        mut reads: ViewReads<'_, '_>,
        mut commands: Commands<'_, '_>,
        mut views: Local<'_, Vec<Request>>,
    ) {
        let now = reads.time.elapsed();
        let tick = (reads.received.get()).and_then(|next| next.get().checked_sub(1));
        console.take_views(&mut views);
        for request in views.drain(..) {
            let answer = match request.ask {
                ConsoleAsk::Camera(ask) => Some(reads.camera(&ask)),
                ConsoleAsk::Screenshot(ask) => match reads.region(&ask.region, shots.last.as_ref())
                {
                    Ok(region) => {
                        let pending = PendingShot {
                            id: request.id,
                            asked: now,
                            region,
                            tick,
                            camera: reads.rig.pose(),
                        };
                        shots.pending.push(pending.clone());
                        commands.spawn(Screenshot::primary_window()).observe(
                            move |captured: Captured<'_, '_>, views: ViewWrites<'_>| {
                                Self::captured(&pending, &captured.image, views);
                            },
                        );
                        None
                    }
                    Err(refusal) => Some(ConsoleAnswer::Refused(refusal)),
                },
                _ => unreachable!("the view takes the requests of the view alone"),
            };
            if let Some(answer) = answer {
                console.answer(Reply {
                    id: request.id,
                    tick,
                    answer,
                });
            }
        }
        let late = shots
            .pending
            .extract_if(.., |shot| now.saturating_sub(shot.asked) > FRAME_WAIT);
        for shot in late {
            console.answer(Reply {
                id: shot.id,
                tick: shot.tick,
                answer: ConsoleAnswer::Refused(ConsoleRefusal::NoFrame),
            });
        }
    }

    /// Answers the screenshot `shot` with its region of the window's `frame`, unless it waited
    /// past its second.
    fn captured(shot: &PendingShot, frame: &Image, (mut console, mut shots): ViewWrites<'_>) {
        let Some(at) = shots
            .pending
            .iter()
            .position(|pending| pending.id == shot.id)
        else {
            return;
        };
        shots.pending.remove(at);
        let answer = match Self::rgba(frame) {
            Some(frame) => {
                let size = UVec2::new(frame.width, frame.height);
                match ShotFrame::of(shot.region, size) {
                    Some(cut) => {
                        let image = cut.image(&frame);
                        shots.last = Some(cut);
                        ConsoleAnswer::Screenshot(WindowShot {
                            png: Base64::encode(&Png::encode(&image)),
                            width: image.width,
                            height: image.height,
                            origin: cut.region().min.to_array(),
                            scale: cut.scale(),
                            camera: shot.camera,
                        })
                    }
                    None => ConsoleAnswer::Refused(ConsoleRefusal::OffScreen),
                }
            }
            None => ConsoleAnswer::Refused(ConsoleRefusal::NoFrame),
        };
        console.answer(Reply {
            id: shot.id,
            tick: shot.tick,
            answer,
        });
    }

    /// The pixels of `frame` as 8-bit RGBA; none for a frame Bevy does not convert.
    fn rgba(frame: &Image) -> Option<RgbaImage> {
        let image = frame.clone().try_into_dynamic().ok()?.into_rgba8();
        Some(RgbaImage {
            width: image.width(),
            height: image.height(),
            pixels: image.into_raw(),
        })
    }
}

impl ViewReads<'_, '_> {
    /// Reads the camera, or moves it as `ask` says, and gives its pose as it then stands.
    fn camera(&mut self, ask: &CameraAsk) -> ConsoleAnswer {
        match ask {
            CameraAsk::Read => {}
            &CameraAsk::Set(pose) => self.rig.set_pose(pose),
            &CameraAsk::Unit(id) => {
                let Some(place) = self.drawn_place(id) else {
                    return ConsoleAnswer::Refused(ConsoleRefusal::UnknownUnit { id });
                };
                self.rig.look_at(Vec2::new(place.x, place.z));
            }
            &CameraAsk::Point { x, z } => self.rig.look_at(Vec2::new(x, z)),
        }
        ConsoleAnswer::Camera(self.rig.pose())
    }

    /// The rectangle of the window's frame `region` names: the whole, a rectangle of the last
    /// screenshot's pixels, or the box round units on the screen.
    fn region(
        &self,
        region: &ShotRegion,
        last: Option<&ShotFrame>,
    ) -> Result<URect, ConsoleRefusal> {
        let frame = self.window.physical_size();
        match region {
            ShotRegion::Window => Ok(URect::from_corners(UVec2::ZERO, frame)),
            &ShotRegion::Pixels {
                x,
                y,
                width,
                height,
            } => {
                let last = last.ok_or(ConsoleRefusal::NoShot)?;
                let pixels = URect::new(x, y, x.saturating_add(width), y.saturating_add(height));
                Ok(last.frame_rect(pixels))
            }
            ShotRegion::Units { ids } => {
                let points: Vec<Vec2> = ids.iter().flat_map(|&id| self.shown_corners(id)).collect();
                ShotFrame::round(points, frame).ok_or(ConsoleRefusal::OffScreen)
            }
        }
    }

    /// Where the unit of `id`'s drawing stands; none for a unit the client holds no drawing of.
    fn drawn_place(&self, id: u64) -> Option<Vec3> {
        let root = self.root(id)?;
        Some(self.places.get(root).ok()?.translation())
    }

    /// The drawing's root of the unit `id`.
    fn root(&self, id: u64) -> Option<Entity> {
        let unit = self.index.get(StableId::from(StableNumber::new(id)))?;
        Some(self.drawings.get(unit).ok()?.root())
    }

    /// The frame's pixels the corners of the bounds of the unit `id`'s drawing show at, those on
    /// the screen.
    fn shown_corners(&self, id: u64) -> Vec<Vec2> {
        let Some(root) = self.root(id) else {
            return Vec::new();
        };
        let (camera, eye) = *self.camera;
        let scale = self.window.scale_factor();
        let parts = std::iter::once(root).chain(self.children.iter_descendants(root));
        let mut shown = Vec::new();
        for (place, aabb) in parts.filter_map(|part| self.bounds.get(part).ok()) {
            let center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            for corner in 0..8_u8 {
                let sign = Vec3::new(
                    if corner & 1 == 0 { -1.0 } else { 1.0 },
                    if corner & 2 == 0 { -1.0 } else { 1.0 },
                    if corner & 4 == 0 { -1.0 } else { 1.0 },
                );
                let world = place.transform_point(center + half * sign);
                if let Ok(at) = camera.world_to_viewport(eye, world) {
                    shown.push(at * scale);
                }
            }
        }
        shown
    }
}
