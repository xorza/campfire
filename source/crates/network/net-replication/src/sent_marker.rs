use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::{AppMarkerExt, AppRuleExt, RuleFns};
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::registry::ctx::{RemoveCtx, WriteCtx};
use bevy_replicon::shared::replication::registry::receive_fns::MutWrite;
use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A marker the sender replicates in place of the one its receiver gets, so an authoritative
/// entity never carries a receiver's marker.
pub(crate) trait SentMarker:
    Component<Mutability: MutWrite<Self>> + Serialize + DeserializeOwned + Sized
{
    /// The marker the receiver gets in its place.
    type Received: Component + Default;

    /// Replicates the marker, which arrives as [`Self::Received`] and leaves with it.
    fn replicate(app: &mut App) {
        app.replicate::<Self>()
            .set_receive_fns::<Self>(Self::write, Self::remove);
    }

    /// Replicon's write of the marker: the received marker in its place.
    fn write(
        ctx: &mut WriteCtx<'_>,
        rule_fns: &RuleFns<Self>,
        entity: &mut DeferredEntity<'_>,
        message: &mut Bytes,
    ) -> Result<()> {
        let _ = rule_fns.deserialize(ctx, message)?;
        entity.insert(Self::Received::default());
        Ok(())
    }

    /// Replicon's removal of the marker: the received marker leaves.
    fn remove(_ctx: &mut RemoveCtx, entity: &mut DeferredEntity<'_>) {
        entity.remove::<Self::Received>();
    }
}
