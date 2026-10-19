/// The clock an `EmulatedNetwork` schedules deliveries on (13-net-stack/time.md, An emulated
/// network has its own wire clock).
///
/// The wire needs a clock of its own: the apps' clocks each count from their own first update,
/// and a harness runs apps at different rates and stalls them, so a sender's clock and a
/// receiver's cannot be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireClock {
    /// The OS time since the network was made, for apps that run in real time in one process.
    Os,
    /// The time the driver sets with
    /// `EmulatedNetwork::set_time` before each frame.
    Driven,
}
