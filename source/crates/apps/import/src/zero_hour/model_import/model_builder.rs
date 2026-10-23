use std::collections::{BTreeMap, BTreeSet};

use bevy_math::Vec3;
use campfire_client_files::{Blend, TeamPaint};

use gltf_json::mesh::{Mode, Primitive, Semantic};
use gltf_json::scene::UnitQuaternion;
use gltf_json::texture::Info;
use gltf_json::validation::Checked;
use gltf_json::{Accessor, Extras, Index, Material, Mesh, Node};

use crate::gltf::glb_builder::GlbBuilder;
use crate::pose::RigidPose;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::axes;
use crate::zero_hour::error::ModelError;
use crate::zero_hour::format::w3d::hierarchy::Hierarchy;
use crate::zero_hour::format::w3d::hlod::SubObject;
use crate::zero_hour::format::w3d::w3d_mesh::{MaterialPass, Stage, VertexMaterial, W3dMesh};
use crate::zero_hour::game_path;
use crate::zero_hour::model_import::model_textures::ModelTextures;
use crate::zero_hour::model_import::w3d_material::{
    self, KeyMaterial, MaterialKey, Source, Surface,
};
use crate::zero_hour::model_nodes::{LookSubObject, ModelNodes, PivotName};

/// A model as it is built: its glTF, its materials by their keys, and the textures no archive
/// holds.
#[derive(Debug)]
pub(super) struct ModelBuilder {
    name: String,
    pub(super) glb: GlbBuilder,
    keys: Vec<MaterialKey>,
    pub(super) missing_textures: BTreeSet<String>,
    /// The files of the textures its meshes draw with, which the import converts.
    pub(super) textures: BTreeSet<ArchivePath>,
}

impl ModelBuilder {
    /// The builder of the model `name`, of nothing yet.
    pub(super) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            glb: GlbBuilder::new(),
            keys: Vec::new(),
            missing_textures: BTreeSet::new(),
            textures: BTreeSet::new(),
        }
    }

    /// An HLOD: each pivot a node in its parent's, and each sub-object a node of its mesh on its
    /// pivot's; a skin's at the scene's root, its vertices already placed.
    pub(super) fn hlod(
        &mut self,
        hierarchy: &Hierarchy,
        sub_objects: &[&SubObject],
        meshes: &[Option<&W3dMesh>],
        textures: &ModelTextures,
    ) -> Result<ModelNodes, ModelError> {
        if sub_objects
            .iter()
            .any(|sub| sub.pivot >= hierarchy.pivots.len())
        {
            return Err(ModelError::Pivot);
        }
        let poses = hierarchy.poses();
        let pivots: Vec<Index<Node>> = hierarchy
            .pivots
            .iter()
            .map(|pivot| {
                let pose = pivot.pose();
                self.glb.document.push(Node {
                    name: Some(pivot.name.clone()),
                    translation: Some(axes::y_up(pose.translation.to_array())),
                    rotation: Some(UnitQuaternion(axes::y_up_rotation(
                        pose.rotation.to_array(),
                    ))),
                    ..Node::default()
                })
            })
            .collect();
        for (&node, pivot) in pivots.iter().zip(&hierarchy.pivots) {
            match pivot.parent {
                Some(parent) => self.child(pivots[parent], node),
                None => self.glb.scene().nodes.push(node),
            }
        }
        let mut nodes = ModelNodes {
            sub_objects: Vec::new(),
            pivots: hierarchy
                .pivots
                .iter()
                .map(|pivot| PivotName {
                    name: pivot.name.clone(),
                    parent: pivot.parent,
                })
                .collect(),
        };
        for (sub, mesh) in sub_objects.iter().zip(meshes) {
            let Some(mesh) = mesh else {
                nodes.sub_objects.push(LookSubObject {
                    name: sub.name.clone(),
                    pivot: sub.pivot,
                    drawn: false,
                    hidden: true,
                });
                continue;
            };
            let mesh_index = self.mesh(mesh, Some(&poses), textures)?;
            let node = self.glb.document.push(Node {
                name: Some(sub.name.clone()),
                mesh: Some(mesh_index),
                ..Node::default()
            });
            if mesh.bones.is_some() {
                self.glb.scene().nodes.push(node);
            } else {
                self.child(pivots[sub.pivot], node);
            }
            nodes.sub_objects.push(LookSubObject {
                name: sub.name.clone(),
                pivot: sub.pivot,
                drawn: true,
                hidden: mesh.is_hidden(),
            });
        }
        // A default look hides nodes by name, so each names one node.
        let mut names = BTreeSet::new();
        if let Some(twice) = self
            .glb
            .document
            .nodes
            .iter()
            .find(|node| !names.insert(&node.name))
        {
            return Err(ModelError::NodeName(
                twice.name.clone().expect("the builder names each node"),
            ));
        }
        Ok(nodes)
    }

    /// Makes `child` a child of `parent`.
    fn child(&mut self, parent: Index<Node>, child: Index<Node>) {
        self.glb.document.nodes[parent.value()]
            .children
            .get_or_insert_with(Vec::new)
            .push(child);
    }

    /// A mesh alone: one node of it, at the scene's root.
    pub(super) fn mesh_alone(
        &mut self,
        mesh: &W3dMesh,
        textures: &ModelTextures,
    ) -> Result<ModelNodes, ModelError> {
        if mesh.bones.is_some() {
            return Err(ModelError::SkinAlone);
        }
        let mesh_index = self.mesh(mesh, None, textures)?;
        let node = self.glb.document.push(Node {
            name: Some(mesh.name.clone()),
            mesh: Some(mesh_index),
            ..Node::default()
        });
        self.glb.scene().nodes.push(node);
        Ok(ModelNodes {
            sub_objects: vec![LookSubObject {
                name: mesh.name.clone(),
                pivot: 0,
                drawn: true,
                hidden: mesh.is_hidden(),
            }],
            pivots: Vec::new(),
        })
    }

    /// A glTF mesh of `mesh`: its vertices on glTF's axes, a skin's placed by each one's bone of
    /// `poses`; for each pass, one primitive for each texture, shader and vertex material its
    /// triangles draw, and for its detail stage one for each texture and shader that mixes it in,
    /// each a layer over what is drawn before it, all of the same vertices (09-zero-hour.md, A W3D
    /// material becomes the one material).
    pub(super) fn mesh(
        &mut self,
        mesh: &W3dMesh,
        poses: Option<&[RigidPose]>,
        textures: &ModelTextures,
    ) -> Result<Index<Mesh>, ModelError> {
        // A glTF mesh needs a primitive; none of the shipped meshes a model draws lacks one.
        if mesh.triangles.is_empty() {
            return Err(ModelError::Empty(mesh.name.clone()));
        }
        let pose_of = |vertex: usize| -> Result<RigidPose, ModelError> {
            match (&mesh.bones, poses) {
                (Some(bones), Some(poses)) => poses
                    .get(usize::from(bones[vertex]))
                    .copied()
                    .ok_or(ModelError::Pivot),
                _ => Ok(RigidPose::IDENTITY),
            }
        };
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut normals = Vec::with_capacity(mesh.normals.len());
        for (vertex, (position, normal)) in mesh.positions.iter().zip(&mesh.normals).enumerate() {
            let pose = pose_of(vertex)?;
            positions.push(axes::y_up(
                pose.place(Vec3::from_array(*position)).to_array(),
            ));
            normals.push(axes::y_up(
                pose.rotate(Vec3::from_array(*normal)).to_array(),
            ));
        }
        Self::fill_zero_normals(&positions, &mesh.triangles, &mut normals);
        let position = self.glb.positions(&positions);
        let normal = self.glb.vectors(&normals);
        let attributes = |texcoord: Option<Index<Accessor>>, color: Option<Index<Accessor>>| {
            [
                (Semantic::Positions, Some(position)),
                (Semantic::Normals, Some(normal)),
                (Semantic::TexCoords(0), texcoord),
                (Semantic::Colors(0), color),
            ]
            .into_iter()
            .filter_map(|(semantic, accessor)| Some((Checked::Valid(semantic), accessor?)))
            .collect()
        };
        let mut primitives = Vec::new();
        // Each pass, and its detail stage, draws over what is drawn before it, one depth bias up.
        let mut depth_bias = 0;
        for pass in &mesh.passes {
            let stage = pass.stage.as_ref();
            let lit =
                (0..mesh.triangles.len()).any(|at| !mesh.shader(pass.shaders.get(at)).is_unlit());
            if pass.colors.is_some() && lit && !Self::colors_are_diffuse(mesh, pass) {
                return Err(ModelError::VertexColors);
            }
            let texcoord = stage.map(|stage| self.coordinates(stage, mesh.positions.len()));
            let color = pass.colors.as_ref().map(|colors| self.glb.colors(colors));
            let mut groups: BTreeMap<(Option<u32>, u32, u32), Vec<u32>> = BTreeMap::new();
            for (at, triangle) in mesh.triangles.iter().enumerate() {
                let shader = pass.shaders.get(at);
                let material = pass
                    .materials
                    .get(usize::try_from(triangle[0]).expect("a u32 fits usize"));
                let texture = stage
                    .filter(|_| mesh.shader(shader).texturing)
                    .and_then(|stage| stage.texture(at));
                groups
                    .entry((texture, shader, material))
                    .or_default()
                    .extend(triangle);
            }
            for ((texture, shader, material), indices) in groups {
                let shader = *mesh.shader(shader);
                let surface = self.surface(mesh, texture, textures);
                let key = MaterialKey {
                    team: Self::team_paint(mesh, true, texture, &surface),
                    surface,
                    source: Source::Pass {
                        shader,
                        material: *mesh.material(material),
                    },
                    two_sided: mesh.is_two_sided(),
                    depth_bias,
                };
                let material = self.material(key)?;
                let indices = self.glb.indices(&indices);
                let color = color.filter(|_| !shader.is_unlit());
                primitives.push(Self::primitive(
                    attributes(texcoord, color),
                    indices,
                    material,
                ));
            }
            depth_bias += 1;
            let Some(detail) = &pass.detail else {
                continue;
            };
            let mut groups: BTreeMap<(Option<u32>, u32), (Blend, Vec<u32>)> = BTreeMap::new();
            for (at, triangle) in mesh.triangles.iter().enumerate() {
                let shader = pass.shaders.get(at);
                let blend = w3d_material::detail_blend(*mesh.shader(shader))
                    .map_err(ModelError::DetailFunction)?;
                if let Some(blend) = blend {
                    groups
                        .entry((detail.texture(at), shader))
                        .or_insert_with(|| (blend, Vec::new()))
                        .1
                        .extend(triangle);
                }
            }
            if groups.is_empty() {
                continue;
            }
            let texcoord = Some(self.coordinates(detail, mesh.positions.len()));
            for ((texture, _), (blend, indices)) in groups {
                let surface = self.surface(mesh, texture, textures);
                let key = MaterialKey {
                    team: Self::team_paint(mesh, false, texture, &surface),
                    surface,
                    source: Source::Detail { blend },
                    two_sided: mesh.is_two_sided(),
                    depth_bias,
                };
                let material = self.material(key)?;
                let indices = self.glb.indices(&indices);
                primitives.push(Self::primitive(
                    attributes(texcoord, None),
                    indices,
                    material,
                ));
            }
            depth_bias += 1;
        }
        Ok(self.glb.document.push(Mesh {
            extensions: None,
            extras: Extras::default(),
            name: Some(mesh.name.clone()),
            primitives,
            weights: None,
        }))
    }

    /// A primitive of triangles of `attributes`, drawn in the order of `indices` with `material`.
    fn primitive(
        attributes: BTreeMap<Checked<Semantic>, Index<Accessor>>,
        indices: Index<Accessor>,
        material: Index<Material>,
    ) -> Primitive {
        Primitive {
            attributes,
            extensions: None,
            extras: Extras::default(),
            indices: Some(indices),
            material: Some(material),
            mode: Checked::Valid(Mode::Triangles),
            targets: None,
        }
    }

    /// Whether the game's vertex colors are its diffuse color, as `MeshMatDescClass` makes them
    /// when its vertex materials use diffuse light, and ambient or not, but no emissive light:
    /// the colors times the diffuse color and opacity, which glTF's colors times its base color
    /// are. With emissive light, the game moves them to another channel. A mesh whose shaders
    /// draw their textures alone uses no vertex colors, so it needs none of this.
    pub(super) fn colors_are_diffuse(mesh: &W3dMesh, pass: &MaterialPass) -> bool {
        let used = |vertex: usize| mesh.material(pass.materials.get(vertex));
        let materials: Vec<&VertexMaterial> = (0..mesh.positions.len()).map(used).collect();
        let lit = |color: fn(&VertexMaterial) -> [u8; 3]| {
            materials.iter().any(|material| color(material) != [0; 3])
        };
        lit(|material| material.diffuse) && !lit(|material| material.emissive)
    }

    /// The coordinates of `stage` over `vertices` vertices. The game reads `V` as `1 − V`, D3D's
    /// texture origin at the top, which is glTF's too. A stage with no coordinates samples `0, 0`
    /// at every vertex, D3D's and glTF's alike; a coordinate that is not finite, which glTF
    /// forbids and the game's GPU samples as it will, is 0 there too.
    fn coordinates(&mut self, stage: &Stage, vertices: usize) -> Index<Accessor> {
        let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
        let flipped: Vec<[f32; 2]> = if stage.coordinates.is_empty() {
            vec![[0.0; 2]; vertices]
        } else {
            stage
                .coordinates
                .iter()
                .map(|&[u, v]| [finite(u), finite(1.0 - v)])
                .collect()
        };
        self.glb.coordinates(&flipped)
    }

    /// How a team colors a material of `mesh`, of a pass's vertex materials if `pass`, of the
    /// texture `id` that draws as `surface`: a texture no archive holds draws as the game's
    /// `MissingTexture`, which no team recolors.
    fn team_paint(mesh: &W3dMesh, pass: bool, id: Option<u32>, surface: &Surface) -> TeamPaint {
        let texture = id
            .filter(|_| matches!(surface, Surface::Texture { .. }))
            .map(|id| {
                mesh.textures[usize::try_from(id).expect("a u32 fits usize")]
                    .name
                    .as_str()
            });
        w3d_material::team_paint(&mesh.name, pass, texture)
    }

    /// What a triangle of the texture `id` of `mesh` is drawn with: nothing for none, the
    /// archives' file of its name, or `MissingTexture` for a name no archive holds, which the
    /// report names.
    fn surface(&mut self, mesh: &W3dMesh, id: Option<u32>, textures: &ModelTextures) -> Surface {
        let Some(id) = id else {
            return Surface::Untextured;
        };
        let texture = &mesh.textures[usize::try_from(id).expect("a u32 fits usize")];
        if let Some(path) = textures.get(&texture.name) {
            self.textures.insert(path.clone());
            Surface::Texture {
                path: path.clone(),
                flags: texture.flags,
            }
        } else {
            self.missing_textures.insert(texture.name.clone());
            Surface::Missing
        }
    }

    /// Gives each vertex whose normal is zero, which glTF forbids, the sum of its triangles'
    /// normals by area, made unit; up if they sum to zero or it has none. The game lights such a
    /// vertex by ambient and emissive alone, which no normal gives.
    pub(super) fn fill_zero_normals(
        positions: &[[f32; 3]],
        triangles: &[[u32; 3]],
        normals: &mut [[f32; 3]],
    ) {
        const UP: [f32; 3] = [0.0, 1.0, 0.0];
        if !normals.contains(&[0.0; 3]) {
            return;
        }
        let zero: Vec<bool> = normals
            .iter()
            .map(|normal| normal.iter().all(|&axis| axis == 0.0))
            .collect();
        for triangle in triangles {
            let corners = triangle.map(|vertex| usize::try_from(vertex).expect("a u32 fits usize"));
            if !corners.iter().any(|&corner| zero[corner]) {
                continue;
            }
            let along = |to: usize| {
                [0, 1, 2].map(|axis| positions[corners[to]][axis] - positions[corners[0]][axis])
            };
            let [first, second] = [along(1), along(2)];
            let face = [
                first[1] * second[2] - first[2] * second[1],
                first[2] * second[0] - first[0] * second[2],
                first[0] * second[1] - first[1] * second[0],
            ];
            for vertex in corners.into_iter().filter(|&vertex| zero[vertex]) {
                for axis in 0..3 {
                    normals[vertex][axis] += face[axis];
                }
            }
        }
        for (normal, _) in normals.iter_mut().zip(&zero).filter(|(_, zero)| **zero) {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            *normal = if length > 0.0 {
                normal.map(|axis| axis / length)
            } else {
                UP
            };
        }
    }

    /// The glTF material of `key`, the model's `<name>_<index>`, made at its first use.
    fn material(&mut self, key: MaterialKey) -> Result<Index<Material>, ModelError> {
        if let Some(at) = self.keys.iter().position(|known| *known == key) {
            return Ok(Index::new(
                u32::try_from(at).expect("a model's materials fit u32"),
            ));
        }
        let name = format!("{}_{}", self.name, self.keys.len());
        let model = game_path::model(&self.name)?;
        let KeyMaterial { mut gltf, texture } = key.convert(&name, &model)?;
        gltf.pbr_metallic_roughness.base_color_texture = texture.map(|texture| Info {
            index: self
                .glb
                .texture(texture.uri, texture.wrap_s, texture.wrap_t),
            tex_coord: 0,
            extensions: None,
            extras: Extras::default(),
        });
        let at = self.glb.material(gltf);
        self.keys.push(key);
        debug_assert_eq!(
            at.value(),
            self.keys.len() - 1,
            "a key is its material's index"
        );
        Ok(at)
    }
}
