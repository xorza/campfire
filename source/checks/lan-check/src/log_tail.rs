use std::io::Read;
use std::path::{Path, PathBuf};

use campfire_log::LogEvent;
use campfire_store::{InputFile, InputStream, PathError, ReadError};

use crate::error::CheckError;
use crate::process::Process;
use crate::process_log::ProcessLog;

/// A process's log as it grows: its file open where the last read ended, so each read takes only
/// what the process wrote since, and the lines read so far.
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
    /// The lines an earlier read gave its caller.
    given: usize,
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
            given: 0,
        }
    }

    /// Each event of type `E` of the lines the process ended since the last read; none while it
    /// made no file.
    pub(crate) fn read<E: LogEvent>(&mut self) -> Result<Vec<E>, CheckError> {
        let file = match &mut self.file {
            Some(file) => file,
            None => match InputFile::stream(&self.path) {
                Ok(file) => self.file.insert(file),
                Err(PathError {
                    error: ReadError::Missing,
                    ..
                }) => return Ok(Vec::new()),
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
        let events = self.log.read_from(self.given)?;
        self.given = self.log.lines().len();
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::Seat;
    use campfire_net::{InputLogged, MatchStarted};
    use campfire_store::AppendWriter;
    use campfire_store::internals::TestDir;
    use campfire_threads::ThreadName;

    use super::*;

    fn started(seat: u8) -> String {
        format!(
            r#"{{"level":"INFO","target":"t","fields":{{"message":"the match started","seat":{seat},"start_tick":5}}}}"#
        )
    }

    #[test]
    fn a_tail_reads_each_line_once_as_its_file_grows() {
        let test_dir = TestDir::new();
        let mut tail = LogTail::new(Process::Bot(0), test_dir.root());
        // No file yet.
        assert_eq!(tail.read::<MatchStarted>().unwrap(), []);
        let path = Process::Bot(0).log_path(test_dir.root());
        let first = format!("{}\n{}", started(1), &started(2)[..20]);
        AppendWriter::create(ThreadName::fixture("tail"), &path, first.as_bytes())
            .unwrap()
            .close();
        // The line the process did not end waits for its end.
        let seat = |seat| MatchStarted {
            seat: Seat::new(seat),
            start_tick: 5,
        };
        assert_eq!(tail.read::<MatchStarted>().unwrap(), [seat(1)]);
        assert_eq!(tail.read::<MatchStarted>().unwrap(), []);
        let len = u64::try_from(first.len()).unwrap();
        let writer = AppendWriter::reopen(ThreadName::fixture("tail"), &path, len).unwrap();
        let more = format!("{}\n{}\n", &started(2)[20..], started(3));
        writer.append(1, |bytes| bytes.extend(more.as_bytes()));
        writer.close();
        // Only the lines after the last read, the one finished among them.
        assert_eq!(tail.read::<MatchStarted>().unwrap(), [seat(2), seat(3)]);
        assert!(tail.read::<InputLogged>().unwrap().is_empty());
        assert_eq!(tail.log.lines().len(), 3);
    }
}
