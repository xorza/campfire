//! A unit's walk of a path as it spawns, a bundle of its own.
use bevy_ecs::bundle::Bundle;
use campfire_sim::StableId;
use campfire_values::PathId;

use crate::on_path::OnPath;
use crate::path_walker::{PathEnd, PathWalker};

/// A unit's walk of a path, as it spawns: the path, and its walker of a spawn group.
#[derive(Bundle, Debug)]
pub struct WalkerOnPath {
    on: OnPath,
    walker: PathWalker,
}

impl WalkerOnPath {
    /// A walk of `path` from its end `from`, as a unit of `group` starts it.
    pub const fn new(path: PathId, from: PathEnd, group: StableId) -> Self {
        Self {
            on: OnPath::new(path),
            walker: PathWalker::start(from, group),
        }
    }
}
