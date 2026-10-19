use crate::event_registration::EventRegistration;
use crate::event_sender::EventSender;
use crate::host_event_sender::HostEventSender;
use crate::message_registry::{
    MessageModeFns, MessageRegistry, ReceiveEventFns, SendFns, SenderFns,
};
use crate::remote_event::receive_event_typed;
use crate::send_queue::SendQueue;
use crate::serialize_fns::CxDeserializeFns;
use crate::serialize_fns::CxSerializeFns;
use crate::serialize_fns::SerializeFns;
use bevy_app::App;
use bevy_ecs::event::Event;
use campfire_net_bytes::ToBytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub trait AppEventExt {
    /// Registers the trigger `M`, which encodes in `codec`'s binary encoding.
    fn register_event<M: Event + Serialize + DeserializeOwned>(
        &mut self,
    ) -> EventRegistration<'_, M>;

    #[doc(hidden)]
    /// Registers the trigger `M`, one of the stack's own types, which encodes as its `ToBytes`
    /// does.
    fn register_event_to_bytes<M: Event + ToBytes>(&mut self) -> EventRegistration<'_, M>;
}

impl AppEventExt for App {
    fn register_event<M: Event + Serialize + DeserializeOwned>(
        &mut self,
    ) -> EventRegistration<'_, M> {
        register_event_with(self, SerializeFns::<M>::default())
    }

    fn register_event_to_bytes<M: Event + ToBytes>(&mut self) -> EventRegistration<'_, M> {
        register_event_with(self, SerializeFns::<M>::with_to_bytes())
    }
}

/// Registers the trigger `M`, which encodes and decodes by `serialize_fns`.
fn register_event_with<M: Event>(
    app: &mut App,
    serialize_fns: SerializeFns<M>,
) -> EventRegistration<'_, M> {
    if app
        .world_mut()
        .get_resource_mut::<MessageRegistry>()
        .is_none()
    {
        app.world_mut().init_resource::<MessageRegistry>();
    }
    let sender_id = app.world_mut().register_component::<EventSender<M>>();
    let host_sender_id = app.world_mut().register_component::<HostEventSender<M>>();

    let mut registry = app.world_mut().resource_mut::<MessageRegistry>();
    registry.register::<M>(
        MessageModeFns::Event {
            send: SendFns {
                link: SenderFns {
                    component_id: sender_id,
                    send_fn: SendQueue::<M>::send::<EventSender<M>>,
                    discard_fn: SendQueue::<M>::discard::<EventSender<M>>,
                },
                host: SenderFns {
                    component_id: host_sender_id,
                    send_fn: SendQueue::<M>::send::<HostEventSender<M>>,
                    discard_fn: SendQueue::<M>::discard::<HostEventSender<M>>,
                },
            },
            receive: ReceiveEventFns {
                receive_event_fn: receive_event_typed::<M>,
            },
        },
        CxSerializeFns::new(serialize_fns.serialize),
        CxDeserializeFns::new(serialize_fns.deserialize),
    );
    EventRegistration {
        app,
        _marker: core::marker::PhantomData,
    }
}
