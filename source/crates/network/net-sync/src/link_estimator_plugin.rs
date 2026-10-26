use bevy_app::{App, Plugin, PreUpdate};
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_connection::Client;
use campfire_net_link::LinkSystems;
use campfire_net_transport::TransportSystems;

use crate::link_estimator::LinkEstimator;

/// The estimate of a client's link to its server, which its timelines read; a server reads none
/// of its clients' links, so it keeps none.
#[derive(Debug)]
pub(crate) struct LinkEstimatorPlugin;

impl Plugin for LinkEstimatorPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Client, LinkEstimator>();
        // The packet session hands its samples over in `LinkSystems::Receive`, and the remote
        // timeline reads the estimate at each packet the transport reads.
        app.add_systems(
            PreUpdate,
            LinkEstimator::sample
                .after(LinkSystems::Receive)
                .before(TransportSystems::Receive),
        );
        app.add_observer(LinkEstimator::handle_connect);
    }
}
