use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use campfire_log::{LogEvent, LogLine};

use crate::error::CheckError;
use crate::process::Process;

/// The lines a process logged, in order.
#[derive(Debug)]
pub(crate) struct ProcessLog {
    process: Process,
    lines: Vec<LogLine>,
}

impl ProcessLog {
    pub(crate) const fn empty(process: Process) -> ProcessLog {
        ProcessLog {
            process,
            lines: Vec::new(),
        }
    }

    /// The log `process` wrote to `path`; empty when it wrote none.
    pub(crate) fn read(process: Process, path: &Path) -> Result<ProcessLog, CheckError> {
        match fs::read_to_string(path) {
            Ok(text) => ProcessLog::parse(process, &text),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(ProcessLog::empty(process)),
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
        let lines = whole
            .lines()
            .enumerate()
            .map(|(index, line)| {
                LogLine::parse(line).map_err(|error| CheckError::Event {
                    process,
                    line: index + 1,
                    error,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(ProcessLog { process, lines })
    }

    pub(crate) fn lines(&self) -> &[LogLine] {
        &self.lines
    }

    /// Every event of type `E`, in order; an error at the first whose fields do not read.
    pub(crate) fn read_all<E: LogEvent>(&self) -> Result<Vec<E>, CheckError> {
        self.events().collect()
    }

    /// The first event of type `E`; an error when its fields do not read.
    pub(crate) fn first<E: LogEvent>(&self) -> Result<Option<E>, CheckError> {
        self.events().next().transpose()
    }

    fn events<E: LogEvent>(&self) -> impl Iterator<Item = Result<E, CheckError>> {
        self.lines.iter().enumerate().filter_map(|(index, line)| {
            line.read::<E>().map(|event| {
                event.map_err(|error| CheckError::Event {
                    process: self.process,
                    line: index + 1,
                    error,
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::PlayerSlot;
    use campfire_net::MatchStarted;

    use super::*;

    #[test]
    fn a_log_drops_its_unfinished_last_line_and_names_a_line_it_cannot_read() {
        let started = r#"{"level":"INFO","target":"t","fields":{"message":"the match started","slot":1,"start_tick":5}}"#;
        let text = format!("{started}\n{}", &started[..20]);
        let log = ProcessLog::parse(Process::Bot(0), &text).unwrap();
        assert_eq!(log.lines().len(), 1);
        assert_eq!(
            log.read_all::<MatchStarted>().unwrap(),
            [MatchStarted {
                slot: PlayerSlot::new(1),
                start_tick: 5
            }]
        );
        // The event of the second line has a field that does not read.
        let flawed = started.replace(r#""slot":1"#, r#""slot":"one""#);
        let log = ProcessLog::parse(Process::Bot(0), &format!("{started}\n{flawed}\n")).unwrap();
        // The first event reads, whatever follows it.
        assert_eq!(
            log.first::<MatchStarted>()
                .unwrap()
                .map(|started| started.slot),
            Some(PlayerSlot::new(1))
        );
        assert!(
            ProcessLog::empty(Process::Server)
                .first::<MatchStarted>()
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            log.read_all::<MatchStarted>(),
            Err(CheckError::Event {
                process: Process::Bot(0),
                line: 2,
                ..
            })
        ));
        // A line that is not JSON fails the parse, by its number.
        assert!(matches!(
            ProcessLog::parse(Process::Server, &format!("{started}\n{{\n")),
            Err(CheckError::Event {
                process: Process::Server,
                line: 2,
                ..
            })
        ));
        assert!(
            ProcessLog::parse(Process::Server, "")
                .unwrap()
                .lines()
                .is_empty()
        );
    }
}
