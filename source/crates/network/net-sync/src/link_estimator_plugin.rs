use bevy_app::{App, Plugin, PreUpdate};
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_connection::{Client, ClientOf};
use campfire_net_link::LinkSystems;
use campfire_net_transport::TransportSystems;

use crate::link_estimator::LinkEstimator;

/// The estimate on each link: a client's to its server, and a server's to each client.
#[derive(Debug)]
pub(crate) struct LinkEstimatorPlugin;

impl Plugin for LinkEstimatorPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Client, LinkEstimator>();
        app.register_required_components::<ClientOf, LinkEstimator>();
        // The session hands its samples over in `LinkSystems::Receive`, and the remote timeline
        // reads the estimate at each packet the transport reads.
        app.add_systems(
            PreUpdate,
            LinkEstimator::sample
                .after(LinkSystems::Receive)
                .before(TransportSystems::Receive),
        );
        app.add_observer(LinkEstimator::handle_connect);
    }
}
