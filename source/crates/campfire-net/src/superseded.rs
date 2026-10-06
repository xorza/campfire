use serde::{Deserialize, Serialize};

/// Tells a client that a newer login of its player took the slot: it stops, and tries its link
/// no more, so the two logins do not take the slot in turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Superseded;
