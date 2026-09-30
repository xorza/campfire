use serde::Deserialize;
use serde_json::Value;

use crate::known_event::KnownEvent;

/// One line of a process's JSON log: its level, the module it came from, its fields with the
/// message among them, and what the check reads of it.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) level: Level,
    pub(crate) target: String,
    pub(crate) fields: Value,
    pub(crate) known: KnownEvent,
}

/// An event's level, as `tracing` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// The part of a line the check reads first.
#[derive(Debug, Deserialize)]
struct Line {
    level: Level,
    target: String,
    fields: Value,
}

impl Event {
    /// The event a line holds; an error when it is not JSON of this shape, or a message the check
    /// reads comes with other fields.
    pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
        let Line {
            level,
            target,
            fields,
        } = serde_json::from_str(line)?;
        let known = KnownEvent::deserialize(&fields)?;
        Ok(Event {
            level,
            target,
            fields,
            known,
        })
    }
}
