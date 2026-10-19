use crate::gltf::gltf_document::GltfDocument;
use crate::pose::Pose;

/// What a model's top is measured from, as the client draws its glTF: each node's name, its
/// children, its place in its parent, and the bounds of each primitive of the mesh it draws, by the
/// accessor's own least and greatest positions, as Bevy's glTF loader gives each its bounds.
#[derive(Debug)]
pub(crate) struct ModelBounds {
    nodes: Vec<BoundsNode>,
    roots: Vec<usize>,
}

/// A node of a model as its top is measured: its name, its children, its place in its parent,
/// and its primitives' bounds, each its least and greatest corner.
#[derive(Debug)]
struct BoundsNode {
    name: String,
    children: Vec<usize>,
    pose: Pose,
    bounds: Vec<[[f32; 3]; 2]>,
}

impl ModelBounds {
    /// The model of `document`'s scene.
    pub(crate) fn of(document: &GltfDocument) -> Self {
        let nodes = document.nodes.iter().map(|node| {
            let primitives = node.mesh.map(|mesh| &document.meshes[mesh].primitives);
            let bounds = primitives.into_iter().flatten().filter_map(|primitive| {
                let positions = &document.accessors[primitive.attributes.position];
                Some([positions.min?, positions.max?])
            });
            BoundsNode {
                name: node.name.clone(),
                children: node.children.clone(),
                pose: Pose {
                    translation: node.translation.unwrap_or(Pose::IDENTITY.translation),
                    rotation: node.rotation.unwrap_or(Pose::IDENTITY.rotation),
                },
                bounds: bounds.collect(),
            }
        });
        Self {
            nodes: nodes.collect(),
            roots: document.scenes[document.scene].nodes.clone(),
        }
    }

    /// The height of the top of what the model draws over its origin, as the client draws it,
    /// before its look's scale: of each primitive's bounds placed by its nodes, the highest
    /// corner, each node `hidden` names hidden with all it holds; none when it draws nothing.
    pub(crate) fn top(&self, hidden: &[String]) -> Option<f32> {
        let mut walk: Vec<(usize, Pose)> = self
            .roots
            .iter()
            .map(|&root| (root, Pose::IDENTITY))
            .collect();
        let mut top = None::<f32>;
        while let Some((at, parent)) = walk.pop() {
            let node = &self.nodes[at];
            if hidden.contains(&node.name) {
                continue;
            }
            let pose = parent.then(node.pose);
            for &bounds in &node.bounds {
                let reach = Self::bounds_top(pose, bounds);
                top = Some(top.map_or(reach, |top| top.max(reach)));
            }
            walk.extend(node.children.iter().map(|&child| (child, pose)));
        }
        top
    }

    /// The height of the highest corner of the box from `bounds[0]` to `bounds[1]` placed by
    /// `pose`: its center's height and its half extents along the rotation's row of the vertical,
    /// in a fixed order, as `Pose` computes.
    fn bounds_top(pose: Pose, [least, most]: [[f32; 3]; 2]) -> f32 {
        let rise = vertical_row(pose.rotation);
        let center = [0, 1, 2].map(|axis| f32::midpoint(least[axis], most[axis]));
        let half = [0, 1, 2].map(|axis| (most[axis] - least[axis]) * 0.5);
        let mut top = pose.translation[1];
        for axis in 0..3 {
            top += rise[axis] * center[axis];
        }
        for axis in 0..3 {
            top += rise[axis].abs() * half[axis];
        }
        top
    }
}

/// The row of the rotation matrix of the unit quaternion `q` that gives a turned vector's height.
fn vertical_row(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    [
        2.0 * (x * y + w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - w * x),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A node of `children`, turned by `rotation` and placed at `translation`, of `bounds`.
    fn node(
        name: &str,
        children: &[usize],
        rotation: [f32; 4],
        translation: [f32; 3],
        bounds: &[[[f32; 3]; 2]],
    ) -> BoundsNode {
        BoundsNode {
            name: name.to_owned(),
            children: children.to_vec(),
            pose: Pose {
                translation,
                rotation,
            },
            bounds: bounds.to_vec(),
        }
    }

    #[test]
    fn a_model_is_as_tall_as_its_shown_bounds_turned_and_placed_by_its_nodes() {
        // A hull 1 m tall at the origin; a turret on it 1 m up, turned a quarter about x, of a box
        // from (0, 0, 0) to (1, 1, 3); and a flash on the turret 0.5 m along its own z, of a box
        // 1 m deep.
        let quarter = std::f32::consts::FRAC_1_SQRT_2;
        let about_x = [quarter, 0.0, 0.0, quarter];
        let identity = Pose::IDENTITY.rotation;
        let model = ModelBounds {
            nodes: vec![
                node(
                    "hull",
                    &[1],
                    identity,
                    [0.0; 3],
                    &[[[-1.0, 0.0, -1.0], [1.0, 1.0, 1.0]]],
                ),
                node(
                    "turret",
                    &[2],
                    about_x,
                    [0.0, 1.0, 0.0],
                    &[[[0.0; 3], [1.0, 1.0, 3.0]]],
                ),
                node(
                    "flash",
                    &[],
                    identity,
                    [0.0, 0.0, 0.5],
                    &[[[0.0; 3], [1.0; 3]]],
                ),
            ],
            roots: vec![0],
        };
        // A quarter about x takes +z to −y and +y to +z: the turret's box spans y from 1 − 3 = −2
        // to 1, and the flash, 0.5 m along the turret's z, at 0.5 m, from 0.5 − 1 to 0.5, so the
        // hull's top at 1 m stands highest. Each within the rounding of a turn by √½.
        let near = |got: Option<f32>, want: f32| got.is_some_and(|got| (got - want).abs() < 1e-6);
        assert!(near(model.top(&[]), 1.0), "{:?}", model.top(&[]));
        // The hull hidden hides all it holds: nothing is drawn.
        assert_eq!(model.top(&["hull".to_owned()]), None);
        // A turret turned the other way, its box's +z up: it spans 1 to 4, over the flash's 1.5 to
        // 2.5; hidden, it hides the flash too, and the hull stands highest.
        let mut up = model;
        up.nodes[1].pose.rotation = [-quarter, 0.0, 0.0, quarter];
        assert!(near(up.top(&[]), 4.0), "{:?}", up.top(&[]));
        assert!(near(up.top(&["turret".to_owned()]), 1.0));
    }
}
