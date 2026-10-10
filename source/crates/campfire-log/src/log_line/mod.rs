use std::fmt::Display;
use std::str::FromStr;

use campfire_common::Json;
use serde::de::DeserializeOwned;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::log_event::LogEvent;

/// One line of a JSON log: the event's level, the module it came from, and its fields, with the
/// message among them.
#[derive(Debug, Deserialize)]
pub struct LogLine {
    pub level: LogLevel,
    pub target: String,
    pub fields: Value,
}

/// An event's level, as `tracing` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLine {
    /// The line `text` holds; an error when it is not a JSON event.
    pub fn parse(text: &str) -> Result<LogLine, serde_json::Error> {
        Json::parse(text)
    }

    /// The event of type `E`, when the line has its message; an error when its fields do not
    /// read as `E`.
    pub fn read<E: LogEvent>(&self) -> Option<Result<E, serde_json::Error>> {
        (self.fields.get("message")?.as_str()? == E::MESSAGE).then(|| E::deserialize(&self.fields))
    }

    /// Reads a field that an event logs with `%`, as its `Display` writes it, back through
    /// `FromStr`: for `#[serde(deserialize_with = "LogLine::text")]`.
    pub fn text<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: FromStr<Err: Display>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }

    /// Reads a field that an event logs as `JsonText`, its JSON in a string, back as a `T`: for
    /// `#[serde(deserialize_with = "LogLine::json")]`.
    pub fn json<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let text = String::deserialize(deserializer)?;
        Json::parse(&text).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests;
