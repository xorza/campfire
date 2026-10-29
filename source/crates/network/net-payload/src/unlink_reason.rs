use core::fmt;

#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::app_code::AppCode;

/// Why a link is unlinked: the request an `Unlink` makes, and the account `Unlinked` keeps from the
/// frame the link unlinks to the frame it links again. An IO layer tells how its connection ended
/// in the cases its protocol can tell apart; one that cannot tell a case reports the nearest.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub enum UnlinkReason {
    /// The link has not linked yet.
    #[default]
    Initial,
    /// This end closed the link with its code.
    Closed(AppCode),
    /// The link's server stopped, which ends every link it holds.
    ServerStopped,
    /// The peer sent nothing for the idle timeout.
    TimedOut,
    /// The peer's endpoint holds no connection of this one: it started again since.
    Reset,
    /// The server refused the connection before its handshake: it has not started, it ended, or
    /// the address holds as many connections or handshakes as its limit lets.
    Refused,
    /// The client refused the server's identity: a raw key it did not pin, or a certificate that
    /// failed its check.
    WrongTlsKey,
    /// The peer's app closed the link with its code.
    ClosedByPeer(AppCode),
    /// The transport protocol failed with this error code, the peer's or this end's.
    Transport { code: u64 },
    /// The endpoint could not open the connection.
    NotOpened,
    /// The endpoint's socket failed: no connection carries anything more.
    SocketFailed,
    /// The peer sent what no honest peer of the same protocol sends: bytes that do not decode,
    /// an unknown channel or message, or a message against its direction.
    ProtocolViolation,
    /// The peer's channels, messages or components differ from this end's.
    ProtocolMismatch,
    /// A reliable message passed the link's send budget, as its peer acknowledges less than it is
    /// sent: one dropped would leave the peer a stream with a hole.
    Backlogged,
    /// A reliable message was larger than its channel's largest, which none dropped may be.
    MessageTooLarge,
}

impl UnlinkReason {
    /// The code a local unlink of this reason closes its link with: its own for a close, the
    /// stack's for a protocol mismatch, which the peer reads as one too, and `ENDED` for any
    /// other.
    pub const fn close_code(self) -> AppCode {
        match self {
            Self::Closed(code) => code,
            Self::ProtocolMismatch => AppCode::PROTOCOL_MISMATCH,
            _ => AppCode::ENDED,
        }
    }

    /// The reason of a link whose peer closed it with `code`: a protocol mismatch the peer found,
    /// or its close.
    pub const fn of_peer_close(code: AppCode) -> Self {
        if code.get() == AppCode::PROTOCOL_MISMATCH.get() {
            Self::ProtocolMismatch
        } else {
            Self::ClosedByPeer(code)
        }
    }
}

impl fmt::Display for UnlinkReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initial => f.write_str("not linked yet"),
            Self::Closed(code) => write!(f, "closed with code {}", code.get()),
            Self::ServerStopped => f.write_str("the server stopped"),
            Self::TimedOut => f.write_str("the peer timed out"),
            Self::Reset => f.write_str("the peer reset the connection"),
            Self::Refused => f.write_str("the server refused the connection"),
            Self::WrongTlsKey => f.write_str("the client refused the server's identity"),
            Self::ClosedByPeer(code) => write!(f, "the peer closed with code {}", code.get()),
            Self::Transport { code } => write!(f, "transport error {code:#x}"),
            Self::NotOpened => f.write_str("the connection could not open"),
            Self::SocketFailed => f.write_str("the socket failed"),
            Self::ProtocolViolation => f.write_str("the peer broke the protocol"),
            Self::Backlogged => {
                f.write_str("the peer acknowledged too little of a reliable stream")
            }
            Self::MessageTooLarge => f.write_str("a reliable message passed its channel's largest"),
            Self::ProtocolMismatch => f.write_str("the peer's protocol differs from this end's"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A close carries its own code, a protocol mismatch the stack's, which the peer reads as a
    /// mismatch too, and any other reason `ENDED`; a peer's other codes read as its close.
    #[test]
    fn a_close_code_reads_back_as_its_reason() {
        let code = AppCode::new(AppCode::FIRST);
        assert_eq!(UnlinkReason::Closed(code).close_code(), code);
        assert_eq!(
            UnlinkReason::ProtocolMismatch.close_code(),
            AppCode::PROTOCOL_MISMATCH
        );
        assert_eq!(UnlinkReason::TimedOut.close_code(), AppCode::ENDED);
        let mismatch = UnlinkReason::ProtocolMismatch.close_code();
        assert_eq!(
            UnlinkReason::of_peer_close(mismatch),
            UnlinkReason::ProtocolMismatch
        );
        for other in [code, AppCode::ENDED, AppCode::DROPPED] {
            assert_eq!(
                UnlinkReason::of_peer_close(other),
                UnlinkReason::ClosedByPeer(other)
            );
        }
    }
}
