use std::path::PathBuf;

use bevy_ecs::resource::Resource;

/// Where the server publishes each session's log: `logs` in its data directory.
#[derive(Resource, Debug, Clone)]
pub(crate) struct LogsDir(pub(crate) PathBuf);
