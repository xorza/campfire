//! Whether this app predicts: what the prediction systems run on.
use crate::prediction_manager::PredictionManager;
use bevy_ecs::prelude::Res;
use campfire_net_connection::NetworkingState;

/// Returns true if this application opted into prediction and has a supported active topology.
pub(crate) fn should_run(
    manager: Option<Res<'_, PredictionManager>>,
    networking: Res<'_, NetworkingState>,
) -> bool {
    predicts(manager.is_some(), &networking)
}

/// Whether this app predicts: it has a prediction manager and is a client. What the prediction
/// systems run on, and what a prediction despawn disables on.
pub(crate) const fn predicts(has_manager: bool, networking: &NetworkingState) -> bool {
    has_manager && networking.topology.is_client()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::App;
    use bevy_ecs::system::RunSystemOnce;
    use campfire_net_connection::NetworkTopology;

    #[test]
    fn prediction_pipeline_excludes_authoritative_topologies() {
        let mut app = App::new();
        app.init_resource::<NetworkingState>();
        app.insert_resource(PredictionManager::default());
        let client = app.world_mut().spawn_empty().id();
        let server = app.world_mut().spawn_empty().id();

        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Client(client);
        assert!(app.world_mut().run_system_once(should_run).unwrap());

        app.world_mut().resource_mut::<NetworkingState>().topology = NetworkTopology::Undefined;
        assert!(!app.world_mut().run_system_once(should_run).unwrap());

        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::HostClient { server, client };
        assert!(!app.world_mut().run_system_once(should_run).unwrap());

        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Server(server);
        assert!(!app.world_mut().run_system_once(should_run).unwrap());
    }
}
