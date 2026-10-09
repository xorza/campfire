use std::path::Path;

use bevy_ecs::resource::Resource;
use campfire_store::{DataDir, DataDirError, PathError};

use crate::sim_server::server_layout::ServerLayout;

/// A server's data directory, which it holds locked while it runs, so no second server writes
/// what it writes; its paths are its layout's.
#[derive(Resource, Debug)]
pub struct ServerDir {
    layout: ServerLayout,
    /// Holds the directory's lock until dropped.
    _data: DataDir,
}

impl ServerDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<ServerDir, PathError<DataDirError>> {
        let data = DataDir::open(path)?;
        Ok(ServerDir {
            layout: ServerLayout::at(data.path().to_owned()),
            _data: data,
        })
    }

    pub const fn layout(&self) -> &ServerLayout {
        &self.layout
    }
}

#[cfg(test)]
mod tests;
