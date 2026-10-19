use crate::message::Message;
use crate::message_error::MessageError;
use crate::message_receive_cx::MessageReceiveCx;
use bevy_ecs::entity::Entity;
use bevy_ecs::event::{EntityEvent, Event};
use bevy_utils::prelude::DebugName;
use campfire_net_core::PeerId;
use tracing::trace;

/// The event a peer triggered, with the peer and the link it arrived on.
#[derive(Event, Debug)]
pub struct RemoteEvent<M: Event> {
    pub event: M,
    pub from: PeerId,
    /// The link entity whose receiver the event landed on: an observer acts on it directly, as a
    /// peer can hold a dying link and a new one at once.
    pub link: Entity,
}

impl<M: EntityEvent> EntityEvent for RemoteEvent<M> {
    fn event_target(&self) -> Entity {
        self.event.event_target()
    }
}

/// Deserializes an event of type `M`, applying receive-side entity mapping, and triggers it as a
/// [`RemoteEvent<M>`].
///
/// # Safety
///
/// `serialize_fns` must be the serialization fns registered for `M`.
pub(crate) fn receive_event_typed<M: Message + Event>(
    cx: MessageReceiveCx<'_, '_, '_>,
) -> Result<(), MessageError> {
    let MessageReceiveCx {
        commands,
        link,
        peer: from,
        reader,
        serialize_fns,
        entity_map,
        ..
    } = cx;
    let trigger = serialize_fns.deserialize::<M>(reader, entity_map)?;
    trace!(
        "Received trigger message: {:?} from: {from:?}",
        DebugName::type_name::<M>()
    );
    commands.command_scope(|mut commands| {
        commands.trigger(RemoteEvent {
            event: trigger,
            from,
            link,
        });
    });
    Ok(())
}
