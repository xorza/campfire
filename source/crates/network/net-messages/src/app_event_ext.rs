use bevy_app::App;
use bevy_ecs::event::Event;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::event_registration::EventRegistration;
use crate::event_sender::EventSender;
use crate::message_registry::{MessageModeFns, MessageRegistry, ReceiveEventFns, SenderFns};
use crate::remote_event::receive_event_typed;
use crate::send_queue::SendQueue;
use crate::serialize_fns::CxDeserializeFns;
use crate::serialize_fns::CxSerializeFns;
use crate::serialize_fns::{default_deserialize, default_serialize};

pub trait AppEventExt {
    /// Registers the trigger `M`, which encodes in `codec`'s binary encoding.
    fn register_event<M: Event + Serialize + DeserializeOwned>(
        &mut self,
    ) -> EventRegistration<'_, M>;
}

impl AppEventExt for App {
    fn register_event<M: Event + Serialize + DeserializeOwned>(
        &mut self,
    ) -> EventRegistration<'_, M> {
        if self
            .world_mut()
            .get_resource_mut::<MessageRegistry>()
            .is_none()
        {
            self.world_mut().init_resource::<MessageRegistry>();
        }
        let sender_id = self.world_mut().register_component::<EventSender<M>>();

        let mut registry = self.world_mut().resource_mut::<MessageRegistry>();
        registry.register::<M>(
            MessageModeFns::Event {
                send: SenderFns {
                    component_id: sender_id,
                    send_fn: SendQueue::<M>::send::<EventSender<M>>,
                    discard_fn: SendQueue::<M>::discard::<EventSender<M>>,
                },
                receive: ReceiveEventFns {
                    receive_event_fn: receive_event_typed::<M>,
                },
            },
            CxSerializeFns::new(default_serialize::<M>),
            CxDeserializeFns::new(default_deserialize::<M>),
        );
        EventRegistration {
            app: self,
            _marker: core::marker::PhantomData,
        }
    }
}
