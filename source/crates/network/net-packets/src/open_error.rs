/// A sealed packet did not authenticate: its tag does not match its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the packet does not authenticate")]
pub struct OpenError;
