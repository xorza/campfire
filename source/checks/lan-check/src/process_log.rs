use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::error::CheckError;
use crate::event::Event;
use crate::known_event::KnownEvent;
use crate::process::Process;

/// The events a process logged, in order.
#[derive(Debug, Default)]
pub(crate) struct ProcessLog {
    events: Vec<Event>,
}

impl ProcessLog {
    /// The log `process` wrote to `path`; empty when it wrote none.
    pub(crate) fn read(process: Process, path: &Path) -> Result<ProcessLog, CheckError> {
        match fs::read_to_string(path) {
            Ok(text) => ProcessLog::parse(process, &text),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(ProcessLog::default()),
            Err(error) => Err(CheckError::File {
                path: path.to_owned(),
                error,
            }),
        }
    }

    /// The log in `text`, one event a line. A last line with no line end is one a process did not
    /// finish writing before it was stopped, and is left out.
    pub(crate) fn parse(process: Process, text: &str) -> Result<ProcessLog, CheckError> {
        let whole = text.rfind('\n').map_or("", |end| &text[..end]);
        let events = whole
            .lines()
            .enumerate()
            .map(|(index, line)| {
                Event::parse(line).map_err(|error| CheckError::Event {
                    process,
                    line: index + 1,
                    error,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(ProcessLog { events })
    }

    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    /// The events the check reads, in order.
    pub(crate) fn known(&self) -> impl Iterator<Item = &KnownEvent> {
        self.events
            .iter()
            .map(|event| &event.known)
            .filter(|known| **known != KnownEvent::Other)
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::PlayerSlot;

    use super::*;

    #[test]
    fn a_log_drops_its_unfinished_last_line_and_names_a_line_it_cannot_read() {
        let started =
            r#"{"level":"INFO","target":"t","fields":{"message":"the match started","slot":1}}"#;
        let text = format!("{started}\n{}", &started[..20]);
        let log = ProcessLog::parse(Process::Bot(0), &text).unwrap();
        assert_eq!(log.events().len(), 1);
        assert_eq!(
            log.known().collect::<Vec<_>>(),
            [&KnownEvent::MatchStarted {
                slot: PlayerSlot::new(1)
            }]
        );
        // A message the check reads, with a field it cannot read: the second line.
        let flawed = started.replace("\"slot\":1", "\"slot\":\"one\"");
        let error = ProcessLog::parse(Process::Bot(0), &format!("{started}\n{flawed}\n"));
        assert!(matches!(
            error,
            Err(CheckError::Event {
                process: Process::Bot(0),
                line: 2,
                ..
            })
        ));
        assert_eq!(
            ProcessLog::parse(Process::Server, "")
                .unwrap()
                .events()
                .len(),
            0
        );
    }
}
