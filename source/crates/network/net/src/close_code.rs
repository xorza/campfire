use campfire_quic::AppCode;
use serde::{Deserialize, Serialize};

campfire_common::enum_all! {
    /// Why Campfire ends a link, which `quic` carries as an `AppCode` and the peer reads back
    /// (15-quic-transport.md, A link's end is a type).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum CloseCode {
        /// The player left the match.
        Left = AppCode::FIRST,
        /// A newer login of the player took the slot.
        Superseded,
        /// The server refused the player's input message.
        InputRefused,
        /// The server's copy of the player's chain is not the client's.
        ChainRewritten,
        /// The session's terms do not fit the client, which cannot play it.
        SessionRefused,
        /// The client sent no answer to the server's offer within its window.
        Unanswered,
        /// The server refused the client's join, which its log names.
        JoinRefused,
    }
    const ALL;
}

impl CloseCode {
    pub const fn app(self) -> AppCode {
        AppCode::new(self as u32)
    }

    /// The code a peer closed with, when it names one of Campfire's.
    pub fn of(code: AppCode) -> Option<Self> {
        Self::ALL.into_iter().find(|close| close.app() == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_code_reads_back_as_itself_and_no_other_number_reads() {
        for close in CloseCode::ALL {
            assert_eq!(CloseCode::of(close.app()), Some(close));
        }
        // Seven codes from 16: 16 to 22.
        let numbers: Vec<u64> = CloseCode::ALL.map(|close| close.app().get()).to_vec();
        assert_eq!(numbers, [16, 17, 18, 19, 20, 21, 22]);
        assert_eq!(CloseCode::of(AppCode::new(23)), None);
        assert_eq!(CloseCode::of(AppCode::ENDED), None);
    }
}
