use bevy_ecs::component::Component;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::units::path_id::PathId;

/// The path a unit belongs to: the one a structure guards, or the one a `PathWalker` walks.
/// `unit.path` reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OnPath(PathId);

impl OnPath {
    pub const fn new(path: PathId) -> OnPath {
        OnPath(path)
    }

    pub const fn get(self) -> PathId {
        self.0
    }
}

impl SimComponent for OnPath {
    const NAME: &'static str = "navigation.on_path";
}
