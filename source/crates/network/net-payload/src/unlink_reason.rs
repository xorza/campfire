#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use core::fmt;
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
            Self::ProtocolMismatch => f.write_str("the peer's protocol differs from this end's"),
        }
    }
}
