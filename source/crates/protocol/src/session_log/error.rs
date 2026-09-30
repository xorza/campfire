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

/// Why bytes do not decode to a session log. A log file is untrusted, so every flaw is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogError {
    /// The format tag is missing.
    NotLog,
    /// The bytes end inside a field.
    Truncated,
    /// A value does not decode.
    Malformed(postcard::Error),
    /// A count of players or inputs, or the payload bytes, do not fit the log's `u32` positions.
    TooLarge,
    /// The log refuses an input logged before `tick`, as it refuses one from the network.
    Input { tick: u64, error: InputError },
    /// The revealed server seed does not match the header's commitment.
    WrongSeed,
    /// Bytes remain after the reveal.
    Trailing,
    /// The bytes decode, but not from the one encoding the log has, such as an overlong varint.
    NotCanonical,
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::NotLog => f.write_str("not a session log"),
            LogError::Truncated => f.write_str("session log ends inside a field"),
            LogError::Malformed(_) => f.write_str("session log value does not decode"),
            LogError::TooLarge => f.write_str("session log above its size bounds"),
            LogError::Input { tick, error } => {
                write!(f, "session log input before tick {tick} refused: {error}")
            }
            LogError::WrongSeed => {
                f.write_str("session log reveals a server seed that does not match its commitment")
            }
            LogError::Trailing => f.write_str("session log has trailing bytes"),
            LogError::NotCanonical => f.write_str("session log is not in its canonical encoding"),
        }
    }
}

impl Error for LogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LogError::Malformed(error) => Some(error),
            LogError::Input { error, .. } => Some(error),
            _ => None,
        }
    }
}
