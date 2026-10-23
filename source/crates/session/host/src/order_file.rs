use std::path::{Path, PathBuf};

use campfire_bots::OrderScript;
use campfire_store::InputFile;

use crate::error::OrderFileError;

/// The file of an order script a bot plays, as a command line names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderFile(PathBuf);

impl OrderFile {
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The script the file holds, read no further than `OrderScript::MAX_FILE_LEN`; an error
    /// for a file that does not read, and for text `OrderScript::parse` refuses.
    pub fn read(&self) -> Result<OrderScript, OrderFileError> {
        let text = InputFile::read_text(&self.0, OrderScript::MAX_FILE_LEN)
            .map_err(OrderFileError::Read)?;
        OrderScript::parse(&text).map_err(|error| OrderFileError::Script {
            path: self.0.clone(),
            error,
        })
    }
}

impl From<PathBuf> for OrderFile {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
