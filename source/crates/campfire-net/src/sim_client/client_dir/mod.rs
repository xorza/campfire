use std::path::Path;

use campfire_store::{DataDir, DataDirError};

use crate::sim_client::client_layout::ClientLayout;

/// A client's data directory, which it holds locked while it runs, so no second client writes
/// what it writes; its paths are its layout's.
#[derive(Debug)]
pub struct ClientDir {
    layout: ClientLayout,
    /// Holds the directory's lock until dropped.
    _data: DataDir,
}

impl ClientDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<ClientDir, DataDirError> {
        let data = DataDir::open(path)?;
        Ok(ClientDir {
            layout: ClientLayout::at(data.path().to_owned()),
            _data: data,
        })
    }

    pub const fn layout(&self) -> &ClientLayout {
        &self.layout
    }
}

#[cfg(test)]
mod tests;
