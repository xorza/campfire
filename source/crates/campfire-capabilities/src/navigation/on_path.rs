use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::navigation::navigation_book::NavigationBook;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::path_id::PathId;
use crate::units::unit_entity::UnitEntity;

/// The path a unit belongs to: the one a structure guards, or the one a `PathWalker` walks.
/// `unit.path` reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct OnPath(PathId);

impl OnPath {
    pub(crate) const fn new(path: PathId) -> OnPath {
        OnPath(path)
    }

    pub(crate) const fn get(self) -> PathId {
        self.0
    }
}

impl SimComponent for OnPath {
    const NAME: &'static str = "navigation.on_path";
    type Host = UnitEntity;

    // A path the map lacks has no name for `unit.path` to read.
    fn check(&self, world: &World, _: Entity) -> bool {
        self.0.index() < world.resource::<NavigationBook>().paths.count() as usize
    }
}

impl Replication for OnPath {
    const KIND: DataKind = DataKind::Prediction;
    type Sending = SentOnce;
}
