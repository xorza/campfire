use std::io::Read;
use std::path::{Path, PathBuf};

use campfire_log::LogEvent;
use campfire_store::{InputFile, InputStream, PathError, ReadError};

use crate::error::CheckError;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// A process's log as it grows: its file open where the last read ended, so each read takes only
/// what the process wrote since, and the lines read so far, each parsed once.
#[derive(Debug)]
pub(crate) struct LogTail {
    path: PathBuf,
    /// None until the process made its file.
    file: Option<InputStream>,
    /// What the file held past its last whole line.
    rest: Vec<u8>,
    /// The bytes read from the file, within `ProcessLog::MAX_LEN`.
    read: usize,
    log: ProcessLog,
}

impl LogTail {
    /// The log `process` writes in `dir`, none of it read yet.
    pub(crate) fn new(process: Process, dir: &Path) -> Self {
        Self {
            path: process.log_path(dir),
            file: None,
            rest: Vec::new(),
            read: 0,
            log: ProcessLog::empty(process),
        }
    }

    /// Each event of type `E` of the lines past the first `seen`, once the lines the process
    /// ended since the last read are read; `seen` then counts every line read. None while it made
    /// no file.
    pub(crate) fn read_past<E: LogEvent>(
        &mut self,
        seen: &mut usize,
    ) -> Result<Vec<E>, CheckError> {
        let log = self.read()?;
        let events = log.read_from(*seen)?;
        *seen = log.lines().len();
        Ok(events)
    }

    /// The log as it is once what the process wrote since the last read is read: the whole log
    /// of a process that ended, but for a last line it did not end.
    pub(crate) fn read(&mut self) -> Result<&ProcessLog, CheckError> {
        let file = match &mut self.file {
            Some(file) => file,
            None => match InputFile::stream(&self.path) {
                Ok(file) => self.file.insert(file),
                Err(PathError {
                    error: ReadError::Missing,
                    ..
                }) => return Ok(&self.log),
                Err(error) => return Err(CheckError::Read(error)),
            },
        };
        let failed = |error| {
            CheckError::Read(PathError {
                path: self.path.clone(),
                error,
            })
        };
        let left = ProcessLog::MAX_LEN - self.read;
        let read = file
            .take(u64::try_from(left).expect("a length fits u64") + 1)
            .read_to_end(&mut self.rest)
            .map_err(|error| failed(ReadError::Read(error)))?;
        if read > left {
            return Err(failed(ReadError::TooLarge {
                max: ProcessLog::MAX_LEN,
            }));
        }
        self.read += read;
        let whole = self.log.extend(&self.rest)?;
        self.rest.drain(..whole);
        Ok(&self.log)
    }
}

#[cfg(test)]
mod tests;
