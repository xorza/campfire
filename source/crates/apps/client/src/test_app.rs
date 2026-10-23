//! The apps a test of the client's systems runs them in.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use bevy::DefaultPlugins;
use bevy::app::{App, PluginGroup};
use bevy::asset::{AssetApp, AssetPlugin};
use bevy::log::LogPlugin;
use bevy::mesh::Mesh;
use bevy::pbr::StandardMaterial;
use bevy::render::RenderPlugin;
use bevy::render::settings::WgpuSettings;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAsset;
use campfire_actions::{ActionBook, SlotKindBook};
use campfire_app_threads::OneThreadPlugins;
use campfire_client_files::{HudFile, PickingFile, TeamColors};
use campfire_geometry::Metric;
use campfire_host::JoinState;
use campfire_sim::{EntityIndex, SimTick, Unpredicted};
use campfire_test_packages::TestMode;
use campfire_units::MapBook;

use crate::input_map::InputMap;
use crate::selection::Selection;
use crate::view::client_data::ClientLoad;
use crate::view::map_metric::MapMetric;
use crate::view::match_colors::MatchColors;
use crate::view::match_hud::MatchHud;
use crate::view::match_picking::MatchPicking;
use crate::view::unit_looks::UnitLooks;

/// An app with no window of what a client's match holds before any plugin of the client's end, as
/// `add_match` adds it, with a clock and the assets of meshes and materials.
pub(crate) fn view() -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    add_match(&mut app);
    app
}

/// An app of Bevy's default plugins, as a window's end adds them, but with no window, no event
/// loop and no render backend.
pub(crate) fn headless() -> App {
    let mut app = App::new();
    let render = RenderPlugin {
        render_creation: WgpuSettings {
            backends: None,
            ..WgpuSettings::default()
        }
        .into(),
        ..RenderPlugin::default()
    };
    let window = WindowPlugin {
        primary_window: None,
        exit_condition: ExitCondition::DontExit,
        ..WindowPlugin::default()
    };
    app.add_plugins(
        DefaultPlugins
            .set(render)
            .set(window)
            .disable::<WinitPlugin>()
            .disable::<LogPlugin>(),
    );
    app
}

/// Adds to `app` what a client's match holds before any plugin of the client's end: the match's
/// books and tick, empty, the world's bounds for its map, the client's join, waiting for its
/// offer, no looks, an empty selection, the engine's keys, and a clock that moves only as a test
/// sets it.
pub(crate) fn add_match(app: &mut App) {
    app.init_resource::<EntityIndex>();
    app.init_resource::<SimTick>();
    app.init_resource::<MapBook>();
    app.init_resource::<ActionBook>();
    app.insert_resource(SlotKindBook(Arc::default()));
    let hz = NonZeroU32::new(30).expect("a rate");
    app.insert_resource(JoinState::waiting(Arc::new(TestMode::Lane.packages()), hz));
    app.init_resource::<Selection>();
    app.init_resource::<InputMap>();
    app.init_resource::<UnitLooks>();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    Unpredicted::register(app.world_mut());
    app.init_resource::<ClientLoad>();
    app.insert_resource(MapMetric(Metric::Planar));
    app.insert_resource(MatchColors::new(TeamColors::default()));
    app.insert_resource(MatchHud(HudFile::default()));
    app.insert_resource(MatchPicking(PickingFile::default()));
}

/// An app with no window that loads assets on the test's thread, models among them.
pub(crate) fn assets() -> App {
    let mut app = App::new();
    app.add_plugins((OneThreadPlugins, AssetPlugin::default()));
    app.init_asset::<WorldAsset>();
    app
}
