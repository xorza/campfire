use std::error::Error;
use std::fmt;

/// Why a server refuses a joining player's connect answer. The answer comes from the player, so
/// each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    /// The delegation names another server.
    OtherServer,
    /// The delegation names another session.
    OtherSession,
    /// The delegation expired by the server's clock.
    Expired,
    /// The delegation's session key did not sign this challenge with this server's certificate
    /// hash: a reply relayed from a connection to another server fails here.
    BadAnswer,
}

/// Text for a certificate hash that is not 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotHex;

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConnectError::OtherServer => "the delegation names another server",
            ConnectError::OtherSession => "the delegation names another session",
            ConnectError::Expired => "the delegation expired",
            ConnectError::BadAnswer => {
                "the session key did not sign this challenge and certificate hash"
            }
        })
    }
}

impl Error for ConnectError {}

impl fmt::Display for NotHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a certificate hash is 64 lowercase hex digits")
    }
}

impl Error for NotHex {}
