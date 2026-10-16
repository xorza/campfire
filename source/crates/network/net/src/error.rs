use campfire_protocol::{ConnectError, DelegationError};
use campfire_runner::ServerInputRefused;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum JoinError {
    #[error(transparent)]
    Delegation(DelegationError),
    #[error(transparent)]
    Connect(ConnectError),
    /// Every slot was taken by the time the join arrived.
    #[error("every slot is taken")]
    Full,
    /// After the start, no slot is the player's, open to them or a bot's they may take.
    #[error("no slot is the player's or open to them")]
    NoSlot,
    /// The log refused what the join would change.
    #[error(transparent)]
    Refused(ServerInputRefused),
    /// A newer login of the player took the link's seat.
    #[error("a newer login of the player took the seat")]
    Superseded,
    /// Every spectator's place the session admits is taken.
    #[error("every spectator's place is taken")]
    NoSpectatorPlace,
}

/// Why a client refused a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ReceiptRefusal {
    /// The client plays no match, nor left one whose link is still open.
    #[error("the client plays no match, nor left one whose link is still open")]
    NotPlaying,
    /// The server key did not sign it.
    #[error("the server key did not sign it")]
    BadSignature,
    /// It names another session, slot or delegation than the player's.
    #[error("it names another session, slot or delegation")]
    Other,
    /// It names a seq the client's history does not hold, or a head that is not the player's.
    #[error("it names a head that is not the player's at its seq")]
    OtherHead,
    /// It names an earlier seq than the one the client keeps.
    #[error("it names an earlier seq than the kept receipt")]
    Older,
}

/// Why a server refused a player's save command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum SaveRefusal {
    /// Only a local server takes a player's saves and loads.
    #[error("only a local server takes a player's saves and loads")]
    NotLocal,
    /// The mode's `[saves] by` is `mode`: it alone saves.
    #[error("the mode alone saves")]
    ByMode,
    /// The server keeps no data to write a save into.
    #[error("the server keeps no data to save into")]
    NoData,
    /// The session holds no save to load.
    #[error("the session holds no save to load")]
    NoSave,
}
