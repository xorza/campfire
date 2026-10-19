use super::*;
use bevy_ecs::system::RunSystemOnce;
use bytes::BytesMut;
use campfire_net_channels::{Channels, PacketKind};
use campfire_net_core::Instant;
use campfire_net_payload::{OpenedPayload, PacketNumber, ReceivePayload};
use core::time::Duration;

#[derive(Resource, Default)]
struct Outcomes {
    received: usize,
    unlinked: Vec<UnlinkReason>,
}

/// A packet that breaks the protocol unlinks its link once, as `ProtocolViolation`, and the
/// systems announce no packet: a short tick with no whole one before it.
#[test]
fn a_protocol_violation_unlinks_the_link_once() {
    let mut app = App::new();
    app.add_plugins(TaskPoolPlugin::default());
    let world = app.world_mut();
    world.insert_resource(Clock::frame_at(Instant::ZERO, Duration::ZERO));
    world.init_resource::<Outcomes>();
    world.add_observer(
        |_: On<'_, '_, PacketReceived>, mut outcomes: ResMut<'_, Outcomes>| {
            outcomes.received += 1;
        },
    );
    world.add_observer(
        |unlink: On<'_, '_, Unlink>, mut outcomes: ResMut<'_, Outcomes>| {
            outcomes.unlinked.push(unlink.reason);
        },
    );
    let mut link = Link::default();
    link.packet_session.push_opened(OpenedPayload {
        number: PacketNumber(0),
        kind: PacketKind::Data.into(),
        payload: ReceivePayload::Mutable(BytesMut::from(&[0_u8, 1][..])),
    });
    world.spawn((link, Linked, Transport::from(Channels::default())));
    world
        .run_system_once(TransportPlugin::buffer_receive)
        .unwrap();
    let outcomes = world.resource::<Outcomes>();
    assert_eq!(outcomes.unlinked, [UnlinkReason::ProtocolViolation]);
    assert_eq!(outcomes.received, 0);
}

/// Stopping a server unlinks its IO.
#[test]
fn stopping_the_server_unlinks_it() {
    use crate::fixtures::transport_stack::TransportStack;
    use campfire_net_connection::Stop;
    use campfire_net_connection::Stopped;
    use campfire_net_link::Unlinked;
    use campfire_net_stepper::StepperSetup;
    use campfire_net_stepper::{StepOrder, Stepper};

    let mut stepper = Stepper::new(TransportStack, StepperSetup::single());
    let server = stepper.server_entity();
    stepper
        .server_app_mut()
        .world_mut()
        .trigger(Stop::from(server));
    stepper.step(10, stepper.frame_interval(), StepOrder::ServerFirst);

    let server = stepper.server();
    assert!(server.contains::<Stopped>());
    assert!(server.contains::<Unlinked>());
    assert!(!server.contains::<Linked>());
}
