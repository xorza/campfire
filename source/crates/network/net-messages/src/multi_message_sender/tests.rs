use super::*;
use crate::fixtures::messages_stack::MessagesStack;
use crate::fixtures::protocol::{ClientMessage, StringEvent, StringMessage};
use crate::fixtures::received_messages::ReceivedMessages;
use crate::remote_event::RemoteEvent;
use bevy_ecs::entity::EntityIndexSet;
use bevy_ecs::entity::{Entity, UniqueEntityArray};
use bevy_ecs::observer::On;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_net_connection::Disconnecting;
use campfire_net_core::PeerId;
use campfire_net_payload::AppCode;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_transport::internals::Channel1;

fn received(stepper: &Stepper<MessagesStack>, client: usize) -> &[(Entity, StringMessage)] {
    &stepper
        .client_world(client)
        .resource::<ReceivedMessages<StringMessage>>()
        .0
}

/// A message to two links reaches each of their clients once.
#[test]
fn a_message_reaches_each_listed_link() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_plain_clients(2));
    for client in 0..2 {
        ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(client));
    }
    let links = [stepper.client_of_entity(0), stepper.client_of_entity(1)];
    let message = StringMessage("World".to_string());
    let sent = message.clone();
    let system =
        stepper
            .server_app_mut()
            .register_system(move |mut sender: MultiMessageSender<'_, '_>| {
                let links = EntityIndexSet::from_iter(links);
                sender.send::<_, Channel1>(&sent, &links);
            });
    stepper.server_world_mut().run_system(system).unwrap();
    stepper.frame_step(2);
    for client in 0..2 {
        let expected = [(stepper.client_entity(client), message.clone())];
        assert_eq!(received(&stepper, client), &expected, "client {client}");
    }
}

#[derive(Resource, Default)]
struct Triggered(Vec<(PeerId, StringEvent)>);

/// An event arrives as a remote event, once on each listed link, from the server.
#[test]
fn an_event_reaches_each_listed_link() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_plain_clients(2));
    for client in 0..2 {
        let app = stepper.client_app_mut(client);
        app.init_resource::<Triggered>();
        app.add_observer(
            |event: On<'_, '_, RemoteEvent<StringEvent>>, mut triggered: ResMut<'_, Triggered>| {
                triggered.0.push((event.from, event.event.clone()));
            },
        );
    }
    let links = [stepper.client_of_entity(0), stepper.client_of_entity(1)];
    let event = StringEvent("Event".to_string());
    let sent = event.clone();
    let system =
        stepper
            .server_app_mut()
            .register_system(move |mut sender: MultiMessageSender<'_, '_>| {
                let links = EntityIndexSet::from_iter(links);
                sender.send::<_, Channel1>(&sent, &links);
            });
    stepper.server_world_mut().run_system(system).unwrap();
    stepper.frame_step(2);
    for client in 0..2 {
        assert_eq!(
            stepper.client_world(client).resource::<Triggered>().0,
            [(PeerId::Server, event.clone())],
            "client {client}"
        );
    }
}

/// A send skips a link that is not connected: here client 1's link is disconnecting, and only
/// client 0 receives.
#[test]
fn all_reaches_only_connected_links() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_plain_clients(2));
    for client in 0..2 {
        ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(client));
    }
    stepper.client_of_mut(1).insert(Disconnecting {
        reason: AppCode::ENDED,
    });
    let message = StringMessage("World".to_string());
    let sent = message.clone();
    let links = [stepper.client_of_entity(0), stepper.client_of_entity(1)];
    let system =
        stepper
            .server_app_mut()
            .register_system(move |mut sender: MultiMessageSender<'_, '_>| {
                let links = EntityIndexSet::from_iter(links);
                sender.send::<_, Channel1>(&sent, &links);
            });
    stepper.server_world_mut().run_system(system).unwrap();
    stepper.frame_step(2);
    assert_eq!(
        received(&stepper, 0),
        &[(stepper.client_entity(0), message)]
    );
    assert_eq!(received(&stepper, 1), &[]);
}

/// A message only a client sends, sent from the server's link to a client, goes against its
/// direction.
#[test]
#[should_panic(expected = "is sent ServerToClient, against its registered direction")]
fn a_message_against_its_direction_is_a_bug() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::single());
    let link = stepper.client_of_entity(0);
    let system =
        stepper
            .server_app_mut()
            .register_system(move |mut sender: MultiMessageSender<'_, '_>| {
                let message = ClientMessage("Server".to_string());
                sender.send::<_, Channel1>(&message, UniqueEntityArray::from([link]));
            });
    stepper.server_world_mut().run_system(system).unwrap();
}
