use serde::de::DeserializeOwned;

/// An event a tool reads back from the JSON log: its message names it, and its fields read into
/// the type. Each one's file tests that `log` writes what the type reads, with
/// `internals::round_trip`.
pub trait LogEvent: DeserializeOwned {
    const MESSAGE: &'static str;

    /// Logs the event at its level, with its fields, under `MESSAGE`.
    fn log(&self);
}
