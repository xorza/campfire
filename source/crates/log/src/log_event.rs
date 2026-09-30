use serde::de::DeserializeOwned;

/// An event a tool reads back from the JSON log: its message names it, and its fields read into
/// the type. Each one's file tests that `log` writes what the type reads, with
/// `internals::round_trip`.
pub trait LogEvent: DeserializeOwned {
    const MESSAGE: &'static str;

    /// Logs the event at its level, with its fields, under `MESSAGE`.
    fn log(&self);
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::fmt::Debug;

    use crate::log_event::LogEvent;
    use crate::log_line::LogLine;
    use crate::logging::internals::capture;

    /// Asserts that `event` logs one JSON line, which reads back as `event`.
    pub fn round_trip<E: LogEvent + PartialEq + Debug>(event: &E) {
        let lines = capture(|| event.log());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = LogLine::parse(&lines[0]).unwrap();
        let read = line.read::<E>().expect("the line is the event");
        assert_eq!(read.as_ref().ok(), Some(event), "{}: {read:?}", lines[0]);
    }
}
