use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, ById};
use campfire_script::MatchNames;
use campfire_values::{ResourceId, UnitType};

use crate::node_data::EmptyNode;
use crate::resource_set::ResourceSet;

/// What each node type holds, and what each drop-off type takes: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct NodeBook {
    pub nodes: ById<UnitType, NodeSpec>,
    pub drop_offs: ById<UnitType, ResourceSet>,
}

/// What a node type holds, and what becomes of its node once it runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpec {
    pub resource: ResourceId,
    pub empty: EmptyNode,
}

impl NodeBook {
    /// The resource a node of `unit_type` holds; `None` for a type that is no node.
    pub(crate) fn resource(&self, unit_type: UnitType) -> Option<ResourceId> {
        self.nodes.get(unit_type).map(|node| node.resource)
    }

    /// Whether a node of `unit_type` stands once it runs out.
    pub(crate) fn stays(&self, unit_type: UnitType) -> bool {
        self.nodes
            .get(unit_type)
            .is_some_and(|node| node.empty == EmptyNode::Stays)
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
