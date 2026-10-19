//! What one link puts on the emulated network for a fixed workload: exact counts for each
//! direction, so a change to the wire shows as a diff of them (13-net-stack/testing.md, Bytes are
//! golden tests, not benches).

use bevy::prelude::*;

use campfire_math::TickRate;
use campfire_net_channels::{Channel, Channels};
use campfire_net_connection::Connected;
use campfire_net_core::LinkDirection;
use campfire_net_emulator::DuplexModel;
use campfire_net_endpoint::Endpoint;
use campfire_net_messages::MessageSender;
use campfire_net_messages::internals::StringMessage;
use campfire_net_packets::EmulatedWires;
use campfire_net_stepper::AppId;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_sync::LinkEstimator;
use campfire_net_transport::Transport;
use campfire_net_transport::internals::{Channel1, ChannelOrderedReliable};
use core::num::NonZeroU32;
use core::ops::Sub;
use core::time::Duration;

use crate::fixtures::full_stack::FullStack;

#[cfg(test)]
mod tests;

/// The tick of every case: 60 Hz, to the nanosecond.
/// The rate the wire costs are measured at, 60 Hz.
pub(crate) const TICK_RATE: TickRate = TickRate::hertz(NonZeroU32::new(60).unwrap());
/// Its tick, 16,666,667 ns, which is also the server's frame interval.
pub(crate) const TICK: Duration = TICK_RATE.period();

/// The length of each message an end sends.
const MESSAGE_BYTES: usize = 200;

/// How long the link runs after it connected and synced, before the window opens.
const WARMUP: Duration = Duration::from_secs(1);

/// How long the counts accumulate.
const WINDOW: Duration = Duration::from_secs(1);

/// What one direction of a link carried in the window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectionCost {
    /// Datagrams put on the wire.
    pub(crate) datagrams: u64,
    /// Bytes of those datagrams.
    pub(crate) bytes: u64,
    /// Of `datagrams`, the packet session's packets that carry only an ACK.
    pub(crate) ack_only: u64,
    /// Of `datagrams`, the packet session's `Ping`s: its keep-alives and probes.
    pub(crate) packet_session_pings: u64,
    /// Messages and fragments sent again.
    pub(crate) resent_messages: u64,
}

impl Sub for DirectionCost {
    type Output = Self;

    fn sub(self, base: Self) -> Self {
        Self {
            datagrams: self.datagrams - base.datagrams,
            bytes: self.bytes - base.bytes,
            ack_only: self.ack_only - base.ack_only,
            packet_session_pings: self.packet_session_pings - base.packet_session_pings,
            resent_messages: self.resent_messages - base.resent_messages,
        }
    }
}

/// What both directions of a link carried in the window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WireCost {
    pub(crate) client_to_server: DirectionCost,
    pub(crate) server_to_client: DirectionCost,
}

impl Sub for WireCost {
    type Output = Self;

    fn sub(self, base: Self) -> Self {
        Self {
            client_to_server: self.client_to_server - base.client_to_server,
            server_to_client: self.server_to_client - base.server_to_client,
        }
    }
}

/// The channel each sending end puts one message on at each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reliability {
    Unreliable,
    Reliable,
}

/// Which ends send at each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Senders {
    Both,
    Server,
}

/// One workload on one link: the server's frame is the tick, and the client's its own.
#[derive(Debug, Clone)]
pub(crate) struct WireCase {
    pub(crate) name: &'static str,
    /// A plain packet session or a sealed one of the UDP protocol.
    pub(crate) kind: ClientKind,
    pub(crate) client_frame: Duration,
    pub(crate) senders: Senders,
    pub(crate) delivery: Reliability,
    pub(crate) link: DuplexModel,
}

/// An end that sends one message at each tick.
#[derive(Component, Debug)]
struct Traffic;

impl WireCase {
    /// The counts of the window, which opens once the link connected, synced and ran `WARMUP`.
    /// Each end takes its RTT samples and sends its heartbeat at the default interval, not in
    /// each frame as the sync stack does for its tests.
    pub(crate) fn measure(&self) -> WireCost {
        let mut setup = StepperSetup::with_clients(vec![self.kind]);
        setup.tick_rate = TICK_RATE;
        setup.frame_interval = TICK;
        setup.link = self.link.clone();
        let mut stepper = Stepper::new(FullStack, setup);
        stepper.set_frame_interval(AppId::Client(0), self.client_frame);
        let defaults = |mut end: EntityWorldMut<'_>| {
            end.insert(LinkEstimator::default());
            end.get_mut::<Transport>()
                .unwrap()
                .set_heartbeat(Channels::DEFAULT_HEARTBEAT);
        };
        defaults(stepper.client_mut(0));
        defaults(stepper.client_of_mut(0));
        let send = match self.delivery {
            Reliability::Unreliable => send_each_tick::<Channel1>,
            Reliability::Reliable => send_each_tick::<ChannelOrderedReliable>,
        };
        stepper.server_app_mut().add_systems(FixedUpdate, send);
        stepper.client_app_mut(0).add_systems(FixedUpdate, send);
        stepper.client_of_mut(0).insert(Traffic);
        if self.senders == Senders::Both {
            stepper.client_mut(0).insert(Traffic);
        }
        stepper.run_for(WARMUP);
        let before = read(&stepper);
        stepper.run_for(WINDOW);
        read(&stepper) - before
    }
}

fn send_each_tick<C: Channel>(
    mut ends: Query<'_, '_, &mut MessageSender<StringMessage>, (With<Traffic>, With<Connected>)>,
) {
    for mut sender in &mut ends {
        sender.send::<C>(StringMessage("x".repeat(MESSAGE_BYTES)));
    }
}

/// The counts of both directions of client 0's link since it was made.
fn read(stepper: &Stepper<FullStack>) -> WireCost {
    let direction = |direction| {
        let network = stepper.network().counts(stepper.client_link(0), direction);
        let (end, endpoint) = match direction {
            LinkDirection::ClientToServer => (stepper.client(0), stepper.client(0)),
            LinkDirection::ServerToClient => (stepper.client_of(0), stepper.server()),
        };
        let packet_session = endpoint
            .get::<Endpoint<EmulatedWires>>()
            .unwrap()
            .packet_session_counts(end.id())
            .unwrap();
        let transport = end.get::<Transport>().unwrap();
        let counts = transport.counts();
        DirectionCost {
            datagrams: network.sent,
            bytes: network.sent_bytes,
            ack_only: packet_session.ack_only,
            packet_session_pings: packet_session.pings,
            resent_messages: counts.resent_messages,
        }
    };
    WireCost {
        client_to_server: direction(LinkDirection::ClientToServer),
        server_to_client: direction(LinkDirection::ServerToClient),
    }
}
