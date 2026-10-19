/// How a stepper client reaches the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// A client in the server's own app, with no link of its own.
    Host,
    /// A client in its own app, in a plain session over the simulated network.
    Raw,
    /// A client in its own app, in a sealed session of the UDP protocol over the simulated
    /// network.
    Udp,
}
