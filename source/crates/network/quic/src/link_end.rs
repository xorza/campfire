use bevy_ecs::component::Component;
use quinn_proto::{ConnectionError, TransportErrorCode};
use serde::{Deserialize, Serialize};

use crate::app_code::AppCode;

/// TLS's `access_denied` alert, which a client's verifier sends for a key it did not pin: QUIC
/// carries it as the crypto error 0x100 + 49 (RFC 9001 §4.8).
const ACCESS_DENIED: u8 = 49;

/// How a link ended, on its entity from the frame it unlinks to the frame it links again
/// (15-quic-transport.md, A link's end is a type): the IO's account of a lost connection, or the
/// code its own end closed it with.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkEnd {
    /// This end closed the link with its code, by `CloseLink`.
    Closed(AppCode),
    /// The peer sent nothing for the idle timeout.
    TimedOut,
    /// The peer's endpoint holds no connection of this one: it started again since.
    Reset,
    /// The server refused the connection before its handshake: it has not started, it ended,
    /// or the address holds as many connections or handshakes as its limit lets
    /// (15-quic-transport.md, Limits for each address, from the session's seats).
    Refused,
    /// The server presented a key the client did not pin.
    WrongServerKey,
    /// The peer's endpoint dropped, as its app exited.
    PeerStopped,
    /// The peer ended the link with no reason given.
    Ended,
    /// The connection's datagrams cannot carry a link's minimum MTU.
    Unfit,
    /// The peer's app closed the link with its code.
    ClosedByPeer(AppCode),
    /// The peer's app closed the link before the peer confirmed the handshake, whose packets
    /// carry no app code (RFC 9000 §10.2.3).
    ClosedInHandshake,
    /// QUIC or TLS failed with this error code, the peer's or this end's.
    Transport { code: u64 },
    /// The peer speaks no QUIC version this end does.
    VersionMismatch,
    /// The endpoint could not open the connection.
    NotOpened,
    /// The endpoint's socket failed, and its thread ended: no connection carries anything more.
    SocketFailed,
}

impl LinkEnd {
    /// How a link whose connection was lost by `error` ended.
    pub(crate) fn of(error: &ConnectionError) -> Self {
        match error {
            ConnectionError::TimedOut => Self::TimedOut,
            ConnectionError::Reset => Self::Reset,
            ConnectionError::VersionMismatch => Self::VersionMismatch,
            ConnectionError::ApplicationClosed(close) => match AppCode::of(close.error_code) {
                AppCode::ENDED => Self::Ended,
                AppCode::STOPPED => Self::PeerStopped,
                AppCode::UNFIT => Self::Unfit,
                code => Self::ClosedByPeer(code),
            },
            ConnectionError::ConnectionClosed(close)
                if close.error_code == TransportErrorCode::CONNECTION_REFUSED =>
            {
                Self::Refused
            }
            ConnectionError::ConnectionClosed(close)
                if close.error_code == TransportErrorCode::APPLICATION_ERROR =>
            {
                Self::ClosedInHandshake
            }
            ConnectionError::ConnectionClosed(close) => Self::Transport {
                code: close.error_code.into(),
            },
            ConnectionError::TransportError(error)
                if error.code == TransportErrorCode::crypto(ACCESS_DENIED) =>
            {
                Self::WrongServerKey
            }
            ConnectionError::TransportError(error) => Self::Transport {
                code: error.code.into(),
            },
            ConnectionError::LocallyClosed => {
                unreachable!("a connection its own end closed has no entity left to tell")
            }
            ConnectionError::CidsExhausted => {
                unreachable!("quinn-proto refuses an accept by it, and loses no connection by it")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use quinn_proto::ConnectionClose;

    use super::*;

    #[test]
    fn a_transport_close_ends_a_link_by_its_code() {
        let closed = |error_code| {
            LinkEnd::of(&ConnectionError::ConnectionClosed(ConnectionClose {
                error_code,
                frame_type: None,
                reason: Bytes::new(),
            }))
        };
        // An app's close in the handshake's packets is 0x0c; a refusal 0x02; a protocol
        // violation, 0x0a, is the transport's.
        assert_eq!(
            closed(TransportErrorCode::APPLICATION_ERROR),
            LinkEnd::ClosedInHandshake
        );
        assert_eq!(
            closed(TransportErrorCode::CONNECTION_REFUSED),
            LinkEnd::Refused
        );
        assert_eq!(
            closed(TransportErrorCode::PROTOCOL_VIOLATION),
            LinkEnd::Transport { code: 0x0a }
        );
    }
}
