use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use campfire_log::{LogEvent, LogLine};
use campfire_net::{InputsDiscarded, OrdersSent};

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

    /// The orders a bot's process sent and kept: each `OrdersSent`, less the last inputs each
    /// `InputsDiscarded` after it drops at a resume, one order an input; an error at the first
    /// event whose fields do not read.
    pub(crate) fn kept_orders(&self) -> Result<Vec<OrdersSent>, CheckError> {
        let failed = |line: usize| {
            move |error| CheckError::Event {
                process: self.process,
                line: line + 1,
                error,
            }
        };
        let mut kept: Vec<OrdersSent> = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            if let Some(sent) = line.read::<OrdersSent>() {
                kept.push(sent.map_err(failed(index))?);
            } else if let Some(discarded) = line.read::<InputsDiscarded>() {
                let mut count = discarded.map_err(failed(index))?.count;
                while let Some(last) = kept.last_mut()
                    && count > 0
                {
                    let dropped = count.min(u64::try_from(last.orders).expect("orders fit u64"));
                    last.orders -= usize::try_from(dropped).expect("at most its orders");
                    count -= dropped;
                    if last.orders == 0 {
                        kept.pop();
                    }
                }
            }
        }
        Ok(kept)
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
mod tests;
