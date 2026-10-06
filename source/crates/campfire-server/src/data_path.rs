use std::path::PathBuf;

use bevy_ecs::resource::Resource;

/// The server's data directory, where it publishes each session's log.
#[derive(Resource, Debug, Clone)]
pub(crate) struct DataPath(pub(crate) PathBuf);
