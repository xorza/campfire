use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, ByType};
use campfire_script::MatchNames;
use campfire_values::{ResourceId, UnitType};

use crate::resource_set::ResourceSet;

/// What each node type holds, and what each drop-off type takes: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct NodeBook {
    pub nodes: ByType<ResourceId>,
    pub drop_offs: ByType<ResourceSet>,
}

impl NodeBook {
    /// The resource a node of `unit_type` holds; `None` for a type that is no node.
    pub(crate) fn resource(&self, unit_type: UnitType) -> Option<ResourceId> {
        self.nodes.get(unit_type).copied()
    }

    /// Whether a unit of `unit_type` takes `resource` as a drop-off.
    pub fn takes(&self, unit_type: UnitType, resource: ResourceId) -> bool {
        self.drop_offs
            .get(unit_type)
            .is_some_and(|taken| taken.contains(resource))
    }
}

impl Book for NodeBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
