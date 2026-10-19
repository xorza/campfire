/// The IO carries nothing more: its socket failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the endpoint's socket failed")]
pub struct IoFailed;
