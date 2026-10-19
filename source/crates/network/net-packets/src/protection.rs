use crate::seal::{Seal, TAG_LEN};

/// How a packet session's packets are protected, which its IO decides: plain on an IO that is
/// in-process or encrypts itself, sealed on one that crosses a network as is.
#[derive(Debug)]
pub enum Protection {
    Plain,
    Sealed(Box<dyn Seal>),
}

impl Protection {
    /// The bytes the protection appends to each packet.
    pub const fn tag_len(&self) -> usize {
        match self {
            Self::Plain => 0,
            Self::Sealed(_) => TAG_LEN,
        }
    }
}
