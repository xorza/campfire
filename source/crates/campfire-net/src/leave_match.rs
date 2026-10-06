use serde::{Deserialize, Serialize};

/// A player's word that they leave the match: the server logs their leave at once, with no grace
/// period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LeaveMatch;
