//! Whether this app predicts: what the prediction systems run on.
use campfire_net_connection::NetworkingState;

/// Whether this app predicts: it has a prediction setup and is a client. What the prediction
/// systems run on, and what a prediction despawn disables on.
pub(crate) const fn predicts(has_setup: bool, networking: &NetworkingState) -> bool {
    has_setup && networking.topology.is_client()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prediction_setup::PredictionSetup;
    use bevy_app::App;
    use bevy_ecs::system::RunSystemOnce;
    use campfire_net_connection::NetworkTopology;

    #[test]
    fn an_authoritative_topology_runs_no_prediction() {
        let mut app = App::new();
        app.init_resource::<NetworkingState>();
        app.insert_resource(PredictionSetup::default());
        let client = app.world_mut().spawn_empty().id();
        let server = app.world_mut().spawn_empty().id();

        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Client(client);
        assert!(
            app.world_mut()
                .run_system_once(PredictionSetup::should_run)
                .unwrap()
        );

        app.world_mut().resource_mut::<NetworkingState>().topology = NetworkTopology::Undefined;
        assert!(
            !app.world_mut()
                .run_system_once(PredictionSetup::should_run)
                .unwrap()
        );

        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Server(server);
        assert!(
            !app.world_mut()
                .run_system_once(PredictionSetup::should_run)
                .unwrap()
        );
    }
}
