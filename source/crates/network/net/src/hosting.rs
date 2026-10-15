/// Where a client's server runs: on another host, or on a thread of the client's own process,
/// the only server that loads a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hosting {
    Remote,
    Local,
}
