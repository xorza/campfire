use std::path::{Path, PathBuf};

use campfire_codec::{Png, RgbaImage, Toml};
use campfire_common::Version;
use campfire_protocol::SessionLog;
use campfire_store::{DurableDir, DurableFile, InputFile};

use crate::client::report_file::ReportFile;
use crate::error::{ReportError, ReportLogError, ReportReadError, ReportWriteError};

/// A report's folder in a client's data directory, `reports/<session id>-<tick>`, a later take
/// of the tick's `-<take>` after it, and the files it holds: its facts, `report.toml`; its
/// screenshot, `screenshot.png`; and a local match's log so far, `session.campfire-log`
/// (02-engine-core.md, A report is a folder of plain files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDir {
    path: PathBuf,
}

/// A report read whole: its facts, and the log of the session they name.
#[derive(Debug)]
pub struct ReadReport {
    pub facts: ReportFile,
    pub log: SessionLog,
}

impl ReportDir {
    /// The report whose folder is `path`, as a reader of a report takes it.
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Makes the folder, and the reports' folder above it, when missing.
    pub fn create(&self) -> Result<(), ReportWriteError> {
        Ok(DurableDir::create_all(&self.path)?)
    }

    /// Writes `facts` durably into the folder.
    pub fn write_facts(&self, facts: &ReportFile) -> Result<(), ReportWriteError> {
        Ok(DurableFile::write(
            &self.facts(),
            Toml::write(facts).as_bytes(),
        )?)
    }

    /// The report the folder holds: its facts and its log, of the session they name; an error
    /// when either does not read, or the log is of another session.
    pub fn read(&self) -> Result<ReadReport, ReportError> {
        let facts = self.read_facts().map_err(ReportError::Facts)?;
        let log = self.read_log().map_err(ReportError::Log)?;
        if log.session_id() != facts.session {
            return Err(ReportError::OtherSession {
                facts: facts.session,
                log: log.session_id(),
            });
        }
        Ok(ReadReport { facts, log })
    }

    /// The facts the folder holds; an error when they do not read, or are of another release,
    /// as `verify_file` refuses a log of one, whose packages need not read in this one.
    fn read_facts(&self) -> Result<ReportFile, ReportReadError> {
        let text = InputFile::read_text(&self.facts(), ReportFile::MAX_LEN)?;
        let facts: ReportFile = Toml::parse(&text)?;
        if facts.release != Version::ENGINE {
            return Err(ReportReadError::OtherRelease(facts.release));
        }
        Ok(facts)
    }

    /// The log the folder holds, decoded; an error when it does not read, as a remote match's
    /// report holds none, or does not decode.
    fn read_log(&self) -> Result<SessionLog, ReportLogError> {
        let bytes = InputFile::read(&self.log(), SessionLog::MAX_FILE_LEN)?;
        Ok(SessionLog::decode(&bytes)?)
    }

    /// Writes `screenshot` durably into the folder as a PNG.
    pub fn write_screenshot(&self, screenshot: &RgbaImage) -> Result<(), ReportWriteError> {
        Ok(DurableFile::write(
            &self.screenshot(),
            &Png::encode(screenshot),
        )?)
    }

    pub fn facts(&self) -> PathBuf {
        self.path.join("report.toml")
    }

    pub fn screenshot(&self) -> PathBuf {
        self.path.join("screenshot.png")
    }

    pub fn log(&self) -> PathBuf {
        self.path.join("session.campfire-log")
    }
}

#[cfg(test)]
mod tests;
