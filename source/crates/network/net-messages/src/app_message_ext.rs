use crate::message::Message;
use crate::message_receiver::MessageReceiver;
use crate::message_registration::MessageRegistration;
use crate::message_registry::{MessageModeFns, MessageRegistry, ReceiveMessageFns, SenderFns};
use crate::message_sender::MessageSender;
use crate::send_queue::SendQueue;
use crate::serialize_fns::CxDeserializeFns;
use crate::serialize_fns::CxSerializeFns;
use crate::serialize_fns::{default_deserialize, default_serialize};
use bevy_app::App;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Add messages or triggers to the list of types that can be sent.
pub trait AppMessageExt {
    /// Register a regular message type `M`.
    /// This registers the sender and default receiver component types. Calling
    /// [`MessageRegistration::add_direction`] installs senders as required
    /// components; receivers are inserted lazily on first receive.
    fn register_message<M: Message + Serialize + DeserializeOwned>(
        &mut self,
    ) -> MessageRegistration<'_, M>;
}

impl AppMessageExt for App {
    fn register_message<M: Message + Serialize + DeserializeOwned>(
        &mut self,
    ) -> MessageRegistration<'_, M> {
        if self
            .world_mut()
            .get_resource_mut::<MessageRegistry>()
            .is_none()
        {
            self.world_mut().init_resource::<MessageRegistry>();
        }
        let sender_id = self.world_mut().register_component::<MessageSender<M>>();
        let receiver_id = self.world_mut().register_component::<MessageReceiver<M>>();

        let mut registry = self.world_mut().resource_mut::<MessageRegistry>();
        registry.register::<M>(
            MessageModeFns::Message {
                send: SenderFns {
                    component_id: sender_id,
                    send_fn: SendQueue::<M>::send::<MessageSender<M>>,
                    discard_fn: SendQueue::<M>::discard::<MessageSender<M>>,
                },
                receive: ReceiveMessageFns {
                    component_id: receiver_id,
                    receive_message_fn: MessageReceiver::<M>::receive_message_typed,
                    message_clear_fn: MessageReceiver::<M>::clear_typed,
                },
            },
            CxSerializeFns::new(default_serialize::<M>),
            CxDeserializeFns::new(default_deserialize::<M>),
        );

        MessageRegistration {
            app: self,
            _marker: core::marker::PhantomData,
        }
    }
}
