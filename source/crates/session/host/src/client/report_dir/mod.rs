use std::path::{Path, PathBuf};

use campfire_codec::{Png, RgbaImage, Toml};
use campfire_store::{DurableDir, DurableFile};

use crate::client::report_file::ReportFile;
use crate::error::ReportWriteError;
use crate::messages::save_command::ReportKey;

/// A report's folder in a client's data directory, `reports/<session id>-<tick>`, a later take
/// of the tick's `-<take>` after it, and the files it holds: its facts, `report.toml`; its
/// screenshot, `screenshot.png`; and a local match's log so far, `session.campfire-log`
/// (bug-reports.md, A report is a folder of plain files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDir {
    path: PathBuf,
    key: ReportKey,
}

impl ReportDir {
    pub(crate) const fn at(path: PathBuf, key: ReportKey) -> Self {
        Self { path, key }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn key(&self) -> ReportKey {
        self.key
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
