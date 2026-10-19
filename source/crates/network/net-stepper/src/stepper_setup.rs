use campfire_math::TickRate;
use campfire_net_emulator::DuplexModel;
use campfire_net_packets::PacketSessionSetup;
use core::num::NonZeroU32;
use core::time::Duration;

use crate::client_kind::ClientKind;

/// The tick rate of a stepper by default, 100 Hz, whose 10 ms period is also its frame interval.
const TICK_RATE: TickRate = TickRate::hertz(NonZeroU32::new(100).unwrap());

/// What `Stepper::new` does once its apps exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Init {
    /// Starts the server, connects every client and steps until each is ready.
    Ready,
    /// Leaves the apps unstarted, for the test to configure and call `Stepper::init`.
    Deferred,
}

/// The settings a `Stepper` starts from.
#[derive(Debug)]
pub struct StepperSetup {
    /// The interval of `Stepper::frame_step`, and of every app's frames in `Stepper::run_for`
    /// until `Stepper::set_frame_interval` changes one.
    pub frame_interval: Duration,
    pub tick_rate: TickRate,
    pub clients: Vec<ClientKind>,
    pub init: Init,
    /// How each remote client's link carries packets; client `i`'s draws follow `seed + i`.
    pub link: DuplexModel,
    /// The packet sessions of every endpoint: their keep-alive, idle timeout and rate.
    pub packet_session: PacketSessionSetup,
    pub seed: u64,
}

impl StepperSetup {
    pub fn with_clients(clients: Vec<ClientKind>) -> Self {
        Self {
            frame_interval: TICK_RATE.period(),
            tick_rate: TICK_RATE,
            clients,
            init: Init::Ready,
            link: DuplexModel::default(),
            packet_session: PacketSessionSetup::default(),
            seed: 0,
        }
    }

    /// One remote client.
    pub fn single() -> Self {
        Self::with_clients(vec![ClientKind::Plain])
    }

    /// One host client and one remote client.
    pub fn host_server() -> Self {
        Self::with_clients(vec![ClientKind::Host, ClientKind::Plain])
    }

    /// `n` remote clients.
    pub fn with_plain_clients(n: usize) -> Self {
        Self::with_clients(vec![ClientKind::Plain; n])
    }
}
