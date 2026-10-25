use std::path::PathBuf;

use campfire_bots::OrderScriptError;
use campfire_codec::TomlError;
use campfire_protocol::{ConnectError, DelegationError};
use campfire_runner::ServerInputRefusal;
use campfire_store::{DurableDirCreateError, DurableDirError, DurableError, PathError, ReadError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why the server refused a player's join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum JoinError {
    #[error(transparent)]
    Delegation(DelegationError),
    #[error(transparent)]
    Connect(ConnectError),
    /// Every player slot was taken by the time the join arrived.
    #[error("every player slot is taken")]
    Full,
    /// After the start, no player slot is the player's, open to them or a bot's they may take.
    #[error("no player slot is the player's or open to them")]
    NoPlayerSlot,
    /// The log refused what the join would change.
    #[error(transparent)]
    Refused(ServerInputRefusal),
    /// A newer login of the player took the link's player slot.
    #[error("a newer login of the player took the player slot")]
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
    /// It names another session, player slot or delegation than the player's.
    #[error("it names another session, player slot or delegation")]
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
    /// The session's log has no room for the load's record.
    #[error("the session's log has no room for a load")]
    LogFull,
    /// The server replays a published log, which it keeps no save of.
    #[error("a replay keeps no save")]
    Replay,
}

/// Why an order script's file does not give a script.
#[derive(Debug, Error)]
pub enum OrderFileError {
    #[error("the order script does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("{} holds no order script", .path.display())]
    Script {
        path: PathBuf,
        #[source]
        error: OrderScriptError,
    },
}

/// Why a report's facts do not read.
#[derive(Debug, Error)]
pub enum ReportReadError {
    #[error(transparent)]
    File(#[from] PathError<ReadError>),
    #[error(transparent)]
    Toml(#[from] TomlError),
}

/// Why a file of a player's report was not written, by its step, each of which its path's error
/// names.
#[derive(Debug, Error)]
pub enum ReportWriteError {
    /// The reports' folder or the report's was not made.
    #[error(transparent)]
    Dir(#[from] PathError<DurableDirError>),
    /// The report's own folder was not made new.
    #[error(transparent)]
    NewDir(#[from] PathError<DurableDirCreateError>),
    #[error(transparent)]
    File(#[from] PathError<DurableError>),
}
