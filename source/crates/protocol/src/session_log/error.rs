use std::error::Error;
use std::fmt;

/// Why the log refused an input. Inputs come from the network, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// The slot is not in the session header.
    UnknownPlayer,
    /// `previous` is not the hash of the player's last logged input: an input is missing, out of
    /// order or altered.
    BrokenLink,
    /// The link holds, but the input does not count on from the player's last one.
    WrongSeq { expected: u64 },
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::UnknownPlayer => f.write_str("input from a player not in the session"),
            InputError::BrokenLink => f.write_str("input does not link to the player's last input"),
            InputError::WrongSeq { expected } => write!(f, "input seq is not {expected}"),
        }
    }
}

impl Error for InputError {}

/// Why a log gives no segment seed. A published log is untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedError {
    /// The server seed is not revealed yet: the segment is not published.
    NotRevealed,
    /// The server seed does not match the header's commitment.
    WrongSeed,
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SeedError::NotRevealed => "the server seed is not revealed",
            SeedError::WrongSeed => "the server seed does not match its commitment",
        })
    }
}

impl Error for SeedError {}
