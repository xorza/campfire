use bevy_app::App;
use bevy_replicon::prelude::{AuthMethod, RepliconPlugins, RepliconSharedPlugin};
use bevy_state::app::StatesPlugin;

/// An app of replicon's shared rules alone, with states and no auth, which a test of a registry
/// or a marker adds its plugins to.
pub fn shared_replicon_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        },
    ));
    app
}

/// An app of replicon's client and server, with states, which a test that writes replicon's
/// messages by hand adds its plugins to.
pub fn replicon_app() -> App {
    let mut app = App::new();
    app.add_plugins((StatesPlugin, RepliconPlugins));
    app
}
