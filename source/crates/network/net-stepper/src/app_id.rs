/// One app of a `Stepper`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppId {
    Server,
    /// The app of remote client `i`.
    Client(usize),
}
