use super::*;
use crate::fixtures::protocol::StringMessage;
use crate::message_receiver::MessageReceiver;
use crate::message_sender::MessageSender;
use bevy_app::Update;
use bevy_ecs::prelude::{Query, ResMut, Resource};
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_transport::internals::Channel1;

#[derive(Resource, Default)]
struct Received(Vec<StringMessage>);

/// The client clears unread messages at the end of each frame, so they are read in it.
fn collect(
    mut receivers: Query<'_, '_, &mut MessageReceiver<StringMessage>>,
    mut received: ResMut<'_, Received>,
) {
    for mut receiver in &mut receivers {
        received.0.extend(receiver.receive());
    }
}

/// A message the server sends reaches the client.
#[test]
fn a_message_reaches_the_client() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::single());
    stepper
        .client_app_mut(0)
        .init_resource::<Received>()
        .add_systems(Update, collect);
    stepper
        .client_of_mut(0)
        .insert(MessageSender::<StringMessage>::default())
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap()
        .send::<Channel1>(StringMessage("hello".to_string()));
    stepper.frame_step(2);
    let received = &stepper.client_world(0).resource::<Received>().0;
    assert_eq!(received, &[StringMessage("hello".to_string())]);
}
