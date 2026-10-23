use crate::component_registry::ComponentRegistry;
use crate::controlled_send::ControlledSend;
use crate::hierarchy;
use crate::replication_target::InterpolatedSend;
use crate::replication_target::PredictedSend;
use crate::sent_marker::SentMarker;
use bevy_app::prelude::{App, Plugin};
use bevy_ecs::prelude::ChildOf;
use bevy_replicon::prelude::{AppMarkerExt, AppRuleExt, RuleFns};

/// Registers the sender-side markers `PredictedSend`, `InterpolatedSend` and `ControlledSend` with
/// Replicon, whose receive makes each receiver's own marker, so no authoritative entity holds a
/// receiver's marker.
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
    }

    fn finish(&self, app: &mut App) {
        app.world_mut().resource_mut::<ComponentRegistry>().seal();
    }
}
