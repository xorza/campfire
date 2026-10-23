/// Where a stack's time comes from: the OS's monotonic clock, for apps that run in real time,
/// or a driver that sets it, as a harness steps its apps on one virtual time
/// (11-net-stack/time.md, An emulated network has its own wire clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSource {
    Os,
    Driven,
}
