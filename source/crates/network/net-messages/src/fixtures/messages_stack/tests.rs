use campfire_net_channels::{Channel, ChannelSetup, ChannelTypeId};
use campfire_net_core::LinkDirections;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_transport::internals::Channel1;
use campfire_net_transport::{AppChannelExt, ChannelRegistry, Transport};

use super::*;
use crate::fixtures::protocol::StringMessage;
use crate::fixtures::received_messages::ReceivedMessages;
use crate::message_sender::MessageSender;

/// A channel its own layer reads.
#[derive(Debug)]
struct Owned;
impl Channel for Owned {}

/// The messages stack with `Owned`, both ways.
#[derive(Debug, Clone, Copy)]
struct OwnedChannelStack;

impl Stack for OwnedChannelStack {
    fn add_client(&self, app: &mut App, client: usize, tick_rate: TickRate) {
        MessagesStack.add_client(app, client, tick_rate);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        MessagesStack.add_server(app, tick_rate);
    }

    fn add_protocol(&self, app: &mut App) {
        MessagesStack.add_protocol(app);
        app.register_channel::<Owned>(ChannelSetup::default())
            .read_by_owner()
            .add_direction(LinkDirections::BOTH);
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        MessagesStack.client_link(link);
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        MessagesStack.server_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        MessagesStack.is_ready(world)
    }
}

/// A message the server sends reaches the client.
#[test]
fn a_message_reaches_the_client() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::single());
    ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(0));
    stepper
        .client_of_mut(0)
        .insert(MessageSender::<StringMessage>::default())
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap()
        .send::<Channel1>(StringMessage("hello".to_string()));
    stepper.frame_step(2);
    let received = &stepper
        .client_world(0)
        .resource::<ReceivedMessages<StringMessage>>()
        .0;
    let link = stepper.client_entity(0);
    assert_eq!(received, &[(link, StringMessage("hello".to_string()))]);
}

/// The message layer reads no channel its owner reads: a message the server sends on `Owned`
/// stays in the client's channel for its owner, and the one on `Channel1` reaches the message
/// layer alone.
#[test]
fn the_message_layer_leaves_a_channel_its_owner_reads() {
    let mut stepper = Stepper::new(OwnedChannelStack, StepperSetup::single());
    ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(0));
    let mut server_link = stepper.client_of_mut(0);
    server_link.insert(MessageSender::<StringMessage>::default());
    let mut sender = server_link
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap();
    sender.send::<Owned>(StringMessage("owned".to_string()));
    sender.send::<Channel1>(StringMessage("read".to_string()));
    stepper.frame_step(2);
    let world = stepper.client_world(0);
    let link = stepper.client_entity(0);
    let received = &world.resource::<ReceivedMessages<StringMessage>>().0;
    assert_eq!(received, &[(link, StringMessage("read".to_string()))]);
    let owned = world
        .resource::<ChannelRegistry>()
        .net_id(ChannelTypeId::of::<Owned>())
        .unwrap();
    let mut client = stepper.client_mut(0);
    let mut transport = client.get_mut::<Transport>().unwrap();
    let receiver = transport.channel_receiver_mut(owned).unwrap();
    assert!(receiver.read_message().is_some());
    assert!(receiver.read_message().is_none());
}
