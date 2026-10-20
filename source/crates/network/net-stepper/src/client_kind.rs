/// How a stepper client reaches the server (11-net-stack/testing.md, Every remote client kind runs
/// the same tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// A client in its own app, in a plain packet session over the emulated network.
    Plain,
    /// A client in its own app, in a sealed packet session of the UDP protocol over the emulated
    /// network.
    Udp,
}
