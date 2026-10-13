use thiserror::Error;

/// Why a server refused a player's save command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
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
