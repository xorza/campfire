use crate::component_registry::ComponentRegistry;
use crate::controlled_send::ControlledSend;
use crate::hierarchy;
use crate::replication_checkpoint_map::ReplicationCheckpointMap;
use crate::replication_target::InterpolatedSend;
use crate::replication_target::PredictedSend;
use crate::sent_marker::SentMarker;
use bevy_app::prelude::{App, Plugin};
use bevy_ecs::prelude::ChildOf;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::{AppMarkerExt, AppRuleExt, RuleFns};

/// Plugin that registers sender-side marker components (`PredictedSend`,
/// `InterpolatedSend`, `ControlledSend`) with Replicon on both client and
/// server. Their custom receive functions materialize the corresponding
/// receiver-local markers without putting those markers on authoritative
/// send-side entities.
#[derive(Debug)]
pub(crate) struct SharedComponentRegistrationPlugin;

impl Plugin for SharedComponentRegistrationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        // The order of app.replicate() calls must be identical on client and server.
        PredictedSend::replicate(app);
        InterpolatedSend::replicate(app);
        ControlledSend::replicate(app);
        // ChildOf is registered for replication in HierarchySendPlugin (server-only),
        // but must also be registered on the client so FnsIds match.
        app.replicate_with(RuleFns::new(
            hierarchy::serialize_child_of,
            hierarchy::deserialize_child_of,
        ))
        .set_receive_fns::<ChildOf>(hierarchy::write_child_of, hierarchy::remove_child_of);

        // ServerMutateTicks is normally only initialized by bevy_replicon's ClientPlugin,
        // but prediction systems on server-only builds also reference it. Init it here
        // so it's always available (defaults to empty/harmless state).
        app.init_resource::<ServerMutateTicks>();
        app.init_resource::<ReplicationCheckpointMap>();
    }

    fn finish(&self, app: &mut App) {
        app.world_mut().resource_mut::<ComponentRegistry>().seal();
    }
}
