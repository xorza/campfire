use gltf_json::scene::UnitQuaternion;
use gltf_json::{Index, Node};

use crate::gltf::glb_builder::GlbBuilder;
use crate::zero_hour::axes;
use crate::zero_hour::format::w3d::hierarchy::Hierarchy;

/// The nodes of a hierarchy's pivots in a glTF, by the pivots' indices: each named as its pivot,
/// at its pivot's pose on glTF's axes in its parent's node, a pivot of no parent in the scene, so
/// a model and an animation of one hierarchy name each node by one path.
#[derive(Debug)]
pub(crate) struct PivotNodes {
    pub(crate) nodes: Vec<Index<Node>>,
}

impl PivotNodes {
    /// The nodes of `hierarchy`'s pivots, pushed into `glb`.
    pub(crate) fn of(hierarchy: &Hierarchy, glb: &mut GlbBuilder) -> Self {
        let nodes: Vec<Index<Node>> = hierarchy
            .pivots
            .iter()
            .map(|pivot| {
                let pose = axes::y_up_pose(pivot.pose());
                glb.document.push(Node {
                    name: Some(pivot.name.clone()),
                    translation: Some(pose.translation.to_array()),
                    rotation: Some(UnitQuaternion(pose.rotation.to_array())),
                    ..Node::default()
                })
            })
            .collect();
        for (&node, pivot) in nodes.iter().zip(&hierarchy.pivots) {
            match pivot.parent {
                Some(parent) => glb.child(nodes[parent], node),
                None => glb.scene().nodes.push(node),
            }
        }
        Self { nodes }
    }
}
