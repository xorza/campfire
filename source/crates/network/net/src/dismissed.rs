use serde::{Deserialize, Serialize};

use crate::close_code::CloseCode;

/// Tells a client that the server ends its link, and why: it stops, tries its link no more, and
/// ends the link itself with the reason's code, which tells the server that the word arrived. A
/// word, not only a close code, as an in-process link carries none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Dismissed {
    /// A newer login of its player took the slot; tried again, the two logins would take the
    /// slot in turn.
    Superseded,
    /// The server refused its join, which the server's log names; a join again would meet the
    /// same refusal.
    JoinRefused,
}

impl Dismissed {
    /// The code the link closes with.
    pub(crate) const fn code(self) -> CloseCode {
        match self {
            Self::Superseded => CloseCode::Superseded,
            Self::JoinRefused => CloseCode::JoinRefused,
        }
    }
}
