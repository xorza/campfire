use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, ById, ByIdBuilder};
use campfire_players::ResourceBook;
use campfire_script::{MatchNames, UnknownName};
use campfire_values::{ResourceId, UnitType};

use crate::Production;
use crate::drop_off_data::DropOffData;
use crate::node_data::{EmptyNode, NodeData};
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

impl NodeBook {
    /// The resource each node type that stands holds and what becomes of its node once it runs
    /// out, and the resources each drop-off type takes; an error for a resource the mode does not
    /// declare.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        let mut nodes = ByIdBuilder::default();
        let mut drop_offs = ByIdBuilder::default();
        for unit in cx.standing() {
            let unknown =
                |name| BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name);
            if let Some(node) = unit.data.section::<NodeData>() {
                let resource = Production::resource(resources, &node.resource).map_err(unknown)?;
                let empty = node.empty;
                nodes.set(unit.unit_type, NodeSpec { resource, empty });
            }
            if let Some(drop_off) = unit.data.section::<DropOffData>() {
                let taken = drop_off
                    .resources
                    .iter()
                    .map(|name| Production::resource(resources, name));
                let taken = taken.collect::<Result<ResourceSet, UnknownName>>();
                drop_offs.set(unit.unit_type, taken.map_err(unknown)?);
            }
        }
        Ok(Self {
            nodes: nodes.build(),
            drop_offs: drop_offs.build(),
        })
    }
}
