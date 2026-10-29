use std::path::PathBuf;

use bevy::app::{App, Plugin, Update};
use bevy::camera::Camera3d;
use bevy::ecs::event::Event;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Res, Single};
use bevy::image::Image;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::transform::components::Transform;
use bevy::window::{PrimaryWindow, Window};
use campfire_common::{ErrorReport, MapName, Tick, Version};
use campfire_host::{ClientLayout, JoinState, NewReport, ReportDir, ReportFile, ReportKey};
use campfire_sim::SimTick;
use campfire_values::PackageName;
use tracing::{error, info};

use crate::input_map::{Act, InputMap};
use crate::selection::{Selection, SelectionSystems};
use crate::view::ViewSystems;
use crate::view::camera_rig::CameraRig;
use crate::window_frame;

/// The report chord, by the key map: it writes a report of the frame the client draws into the
/// client's data directory, its facts and its screenshot, and asks a local match's keys for its
/// server's log (02-engine-core.md, A report is a folder of plain files). As its own resource, what
/// every report of the client holds the same: the data directory it writes into, the mode's name
/// and directory, and the map's name.
#[derive(Resource, Debug, Clone)]
pub(crate) struct ReportPlugin {
    pub(crate) data: ClientLayout,
    pub(crate) mode: PackageName,
    pub(crate) mode_dir: PathBuf,
    pub(crate) map: MapName,
}

/// A report the chord wrote, of the last tick the client ran, whose end it draws.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReportTaken {
    pub(crate) key: ReportKey,
}

impl Plugin for ReportPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.clone());
        app.add_systems(
            Update,
            Self::press
                .run_if(InputMap::on(Act::Report))
                .after(ViewSystems)
                .after(SelectionSystems),
        );
    }
}

impl ReportPlugin {
    /// Writes a report of the frame once the chord is pressed in a match, after the camera and the
    /// selection took their frame's state: its facts at once, and its screenshot once the
    /// renderer captured the frame.
    fn press(
        (reporter, state, tick): (Res<'_, Self>, Res<'_, JoinState>, Res<'_, SimTick>),
        (selection, rig): (Res<'_, Selection>, Res<'_, CameraRig>),
        camera: Single<'_, '_, &Transform, With<Camera3d>>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(session) = state.session() else {
            info!("no match to report");
            return;
        };
        let Some(ran) = tick.start().get().checked_sub(1).map(Tick::new) else {
            info!("no tick of the match ran to report");
            return;
        };
        let facts = ReportFile {
            release: Version::ENGINE,
            mode: reporter.mode.clone(),
            mode_dir: reporter.mode_dir.clone(),
            map: reporter.map.clone(),
            session,
            tick: ran,
            player_slot: state.player_slot(),
            selected: selection.get().to_vec(),
            camera: rig.report(&camera),
            window: window.physical_size().to_array(),
        };
        let written = (reporter.data.create_report_dir(session, facts.tick))
            .and_then(|report| report.dir.write_facts(&facts).map(|()| report));
        let NewReport { dir, key } = match written {
            Ok(report) => report,
            Err(problem) => {
                error!(session = %session, tick = %facts.tick, error = %ErrorReport::of(&problem), "could not write the report");
                return;
            }
        };
        info!(dir = %dir.path().display(), "wrote the report");
        commands.trigger(ReportTaken { key });
        commands.spawn(Screenshot::primary_window()).observe(
            move |captured: On<'_, '_, ScreenshotCaptured>| {
                Self::write_screenshot(&dir, &captured.image);
            },
        );
    }

    /// Writes `frame`, the window's, into the report `dir` as its screenshot; logs a frame that
    /// does not convert or write.
    fn write_screenshot(dir: &ReportDir, frame: &Image) {
        let file = dir.screenshot();
        let image = match window_frame::rgba(frame) {
            Ok(image) => image,
            Err(problem) => {
                error!(file = %file.display(), error = %ErrorReport::of(&problem), "the frame does not convert");
                return;
            }
        };
        match dir.write_screenshot(&image) {
            Ok(()) => info!(file = %file.display(), "wrote the screenshot"),
            Err(problem) => {
                error!(file = %file.display(), error = %ErrorReport::of(&problem), "could not write the screenshot");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    use campfire_codec::Png;
    use campfire_protocol::SessionId;
    use campfire_store::InputFile;
    use campfire_store::internals::TestDir;

    use super::*;

    #[test]
    fn a_frame_of_the_swapchain_s_order_writes_as_an_rgba_png() {
        // Two pixels in the swapchain's BGRA: a red one, then a blue one seen half through.
        let frame = Image::new(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0, 0, 255, 255, 255, 0, 0, 128],
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let test_dir = TestDir::new();
        let dir = ClientLayout::at(test_dir.path("client"))
            .create_report_dir(SessionId::new([4; 32]), Tick::new(7))
            .unwrap()
            .dir;
        ReportPlugin::write_screenshot(&dir, &frame);
        let file = InputFile::read(&dir.screenshot(), 1 << 10).unwrap();
        let image = Png::decode(&file).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, [255, 0, 0, 255, 0, 0, 255, 128]);
    }
}
