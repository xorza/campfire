use crate::audit::Audit;
use crate::fixtures::LINK;
use bevy::prelude::{App, Query, ResMut, Resource, Update};
use campfire_net_channels::Channel;
use campfire_net_messages::MessageReceiver;
use campfire_net_messages::MessageSender;
use campfire_net_messages::internals::protocol::StringMessage;
use campfire_net_stack::internals::full_stack::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;
use campfire_net_transport::internals::protocol::{Channel1, ChannelOrderedReliable};

#[derive(Debug, Default, Resource)]
struct Received(usize);

fn count(
    mut receivers: Query<'_, '_, &mut MessageReceiver<StringMessage>>,
    mut received: ResMut<'_, Received>,
) {
    for mut receiver in &mut receivers {
        received.0 += receiver.receive().count();
    }
}

fn received(app: &App) -> usize {
    app.world().resource::<Received>().0
}

fn send_both_ways<C: Channel>(stepper: &mut Stepper<FullStack>) {
    stepper
        .client_of_mut(0)
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap()
        .send::<C>(StringMessage(String::new()));
    stepper
        .client_mut(0)
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap()
        .send::<C>(StringMessage(String::new()));
    stepper.frame_step(1);
}

#[track_caller]
fn audit_both_ways<C: Channel>(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperConfig::with_clients(vec![kind]));
    for app in stepper.apps_mut() {
        app.init_resource::<Received>().add_systems(Update, count);
    }
    send_both_ways::<C>(&mut stepper);
    Audit::new().budget(LINK).run(|| {
        let server = received(stepper.server_app_mut());
        let client = received(stepper.client_app_mut(0));
        send_both_ways::<C>(&mut stepper);
        assert_eq!(received(stepper.server_app_mut()), server + 1);
        assert_eq!(received(stepper.client_app_mut(0)), client + 1);
    });
}

/// Each frame sends one empty message each way on an unreliable channel, and each side receives
/// one: the server the client's of this frame, as the client runs first, and the client the
/// server's of the frame before, so the first frame runs before the audit. The messages allocate
/// nothing over the idle link.
#[test]
fn messages_both_ways() {
    audit_both_ways::<Channel1>(ClientKind::Raw);
}

/// The same in a sealed session of the UDP protocol.
#[test]
fn sealed_messages_both_ways() {
    audit_both_ways::<Channel1>(ClientKind::Udp);
}

/// The same on an ordered reliable channel, which keeps each message until its packet is
/// delivered.
#[test]
fn reliable_messages_both_ways() {
    audit_both_ways::<ChannelOrderedReliable>(ClientKind::Raw);
}

/// The same in a sealed session of the UDP protocol.
#[test]
fn sealed_reliable_messages_both_ways() {
    audit_both_ways::<ChannelOrderedReliable>(ClientKind::Udp);
}
