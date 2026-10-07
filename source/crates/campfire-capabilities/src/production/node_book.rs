use bevy_ecs::resource::Resource;

use crate::players::resource_id::ResourceId;
use crate::production::resource_set::ResourceSet;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// What each node type holds, and what each drop-off type takes: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NodeBook {
    pub(crate) nodes: ByType<ResourceId>,
    pub(crate) drop_offs: ByType<ResourceSet>,
}

impl NodeBook {
    /// The resource a node of `unit_type` holds; `None` for a type that is no node.
    pub(crate) fn resource(&self, unit_type: UnitType) -> Option<ResourceId> {
        self.nodes.get(unit_type).copied()
    }

    /// Whether a unit of `unit_type` takes `resource` as a drop-off.
    pub(crate) fn takes(&self, unit_type: UnitType, resource: ResourceId) -> bool {
        self.drop_offs
            .get(unit_type)
            .is_some_and(|taken| taken.contains(resource))
    }
}
