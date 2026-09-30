use std::fmt::Display;
use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::log_event::LogEvent;

/// One line of a JSON log: the event's level, the module it came from, and its fields, with the
/// message among them.
#[derive(Debug, Deserialize)]
pub struct LogLine {
    pub level: Level,
    pub target: String,
    pub fields: Value,
}

/// An event's level, as `tracing` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLine {
    /// The line `text` holds; an error when it is not a JSON event.
    pub fn parse(text: &str) -> Result<LogLine, serde_json::Error> {
        serde_json::from_str(text)
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
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use tracing::info;

    use super::*;
    use crate::log_event::internals::round_trip;

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    struct Opened {
        port: u16,
        #[serde(deserialize_with = "LogLine::text")]
        address: Ipv4Addr,
    }

    impl LogEvent for Opened {
        const MESSAGE: &'static str = "opened";

        fn log(&self) {
            info!(port = self.port, address = %self.address, "{}", Self::MESSAGE);
        }
    }

    #[test]
    fn a_line_reads_as_the_event_of_its_message_only() {
        round_trip(&Opened {
            port: 4433,
            address: Ipv4Addr::new(192, 168, 0, 4),
        });
        let line = |fields: &str| {
            LogLine::parse(&format!(
                r#"{{"level":"INFO","target":"t","fields":{fields}}}"#
            ))
            .unwrap()
        };
        let other = line(r#"{"message":"closed","port":1}"#);
        assert_eq!((other.level, other.target.as_str()), (Level::Info, "t"));
        assert!(other.read::<Opened>().is_none());
        // The message names the event, and its fields do not read: an address with a fifth part.
        let flawed = line(r#"{"message":"opened","port":1,"address":"1.2.3.4.5"}"#);
        assert!(matches!(flawed.read::<Opened>(), Some(Err(_))));
        assert!(LogLine::parse("{").is_err());
        assert!(Level::Warn > Level::Info && Level::Error > Level::Warn);
    }
}
