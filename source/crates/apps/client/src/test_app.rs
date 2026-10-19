//! The app a test of the client's systems runs them in.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin};
use bevy::mesh::Mesh;
use bevy::pbr::StandardMaterial;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use campfire_actions::{ActionBook, SlotKindBook};
use campfire_net::JoinState;
use campfire_sim::{EntityIndex, SimTick, Unpredicted};
use campfire_test_packages::TestMode;
use campfire_units::MapBook;

use crate::input_map::InputMap;
use crate::selection::Selection;
use crate::view::client_data::ClientData;
use crate::view::unit_looks::UnitLooks;

/// An app with no window of what a client's match holds before any plugin of the client's end: the
/// match's books and tick, empty, the world's bounds for its map, the client's join, waiting for
/// its offer, no looks, an empty selection, the engine's keys, and a clock that moves only as a
/// test sets it.
pub(crate) fn client_app() -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
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
    app.init_resource::<ClientData>();
    app
}
