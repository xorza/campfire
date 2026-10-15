use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use campfire_client_files::{Blend, MaterialFile, MaterialTexture};

use crate::gltf::glb_builder::{CLAMP_TO_EDGE, GlbBuilder, REPEAT};
use crate::gltf::gltf_document::{
    Attributes, Material, MaterialExtensions, Mesh, Node, Pbr, Primitive, Specular, TextureRef,
    Unlit,
};
use crate::texture::srgb::Srgb;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::{ModelError, ZeroHourError};
use crate::zero_hour::hierarchy::Hierarchy;
use crate::zero_hour::hlod::SubObject;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_parts::{ModelParts, Part, PivotName};
use crate::zero_hour::pose::Pose;
use crate::zero_hour::w3d_mesh::{Pass, Shader, Stage, VertexMaterial, W3dMesh};

/// A model the objects name, converted: its glTF, the material files its shaders need, the
/// parts its default look reads, and the textures its meshes name that no archive holds, which
/// it draws as the game's `MissingTexture`.
#[derive(Debug)]
pub(crate) struct ModelImport {
    pub(crate) glb: GlbBuilder,
    pub(crate) materials: Vec<NamedMaterial>,
    pub(crate) parts: ModelParts,
    pub(crate) missing_textures: BTreeSet<String>,
}

/// What a model's name gives, as `WW3DAssetManager::Create_Render_Obj` finds it.
#[derive(Debug)]
pub(crate) enum Converted {
    Model(Box<ModelImport>),
    /// `NULL`, or a collision box alone: a render object that draws nothing.
    Nothing,
    /// A particle emitter, which the importer does not convert.
    Emitter,
    /// No file of the install holds it.
    Missing,
}

/// A material file, by the name of the glTF material it overrides.
#[derive(Debug)]
pub(crate) struct NamedMaterial {
    pub(crate) name: String,
    pub(crate) file: MaterialFile,
}

/// What makes one material of a model: its surface, what colors it, its two-sidedness and its
/// depth bias, its layer's order over the pass it covers, so equal ones share a material.
#[derive(Debug, Clone, PartialEq)]
struct MaterialKey {
    surface: Surface,
    source: Source,
    two_sided: bool,
    depth_bias: i32,
}

/// What colors a material: a pass's shader and vertex material, or a detail stage's function,
/// whose texture is drawn alone over the pass by its blend.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    Pass {
        shader: Shader,
        material: VertexMaterial,
    },
    Detail {
        blend: Blend,
    },
}

/// What a material's triangles are drawn with: no texture; a texture's path and wraps; or a
/// texture no archive holds, which the game draws as `MissingTexture`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Surface {
    Untextured,
    Texture { path: ArchivePath, flags: u16 },
    Missing,
}

/// Every material's metallic, roughness and specular factors: a W3D material is lit by its
/// colors alone, with no reflection, as no shader of the game turns its specular on.
const METALLIC: f32 = 0.0;
const ROUGHNESS: f32 = 1.0;
const SPECULAR: f32 = 0.0;

/// The render object `Find_Prototype` always has, which draws nothing.
const NULL: &str = "null";

/// `MissingTexture`'s every texel, `0x7FFF00FF`, as glTF's linear factors: red and blue full, no
/// green, and an alpha of 127 of 255, which no sRGB curve bends.
const MISSING: [f32; 4] = [1.0, 0.0, 1.0, 127.0 / 255.0];

/// A texture's clamp flags, `W3DTEXTURE_CLAMP_U` and `_V`.
const CLAMP_U: u16 = 0x0008;
const CLAMP_V: u16 = 0x0010;

impl ModelImport {
    /// The model `name`, a render object's name as an object writes it: `NULL`, or a render
    /// object of that name in the file of the name's part before its first `.`.
    pub(crate) fn convert(
        name: &str,
        assets: &mut ModelAssets<'_>,
    ) -> Result<Converted, ZeroHourError> {
        let name = name.to_ascii_lowercase();
        if name == NULL {
            return Ok(Converted::Nothing);
        }
        let stem = name
            .split('.')
            .next()
            .expect("split gives one part at least");
        let error = |error| ZeroHourError::Model {
            model: name.clone(),
            error,
        };
        let Some(file) = assets.file(stem)? else {
            return Ok(Converted::Missing);
        };
        let mesh_named = |full: &str| file.meshes.iter().find(|mesh| mesh.name == full).cloned();
        // `HLodClass` skips a sub-object no render object has, and keeps a box, which the game
        // never draws, as no code sets `Set_Box_Display_Mask`.
        let (hierarchy_name, sub_objects, meshes) =
            if let Some(hlod) = file.hlods.iter().find(|hlod| hlod.name == name) {
                let mut sub_objects = Vec::with_capacity(hlod.sub_objects.len());
                let mut meshes = Vec::with_capacity(hlod.sub_objects.len());
                for sub in &hlod.sub_objects {
                    let mesh = mesh_named(&sub.name);
                    if mesh.is_some() || file.boxes.contains(&sub.name) {
                        sub_objects.push(sub.clone());
                        meshes.push(mesh);
                    }
                }
                (Some(hlod.hierarchy.clone()), sub_objects, meshes)
            } else if let Some(mesh) = mesh_named(&name) {
                (None, Vec::new(), vec![Some(mesh)])
            } else if file.boxes.contains(&name) {
                return Ok(Converted::Nothing);
            } else if file.emitters.contains(&name) {
                return Ok(Converted::Emitter);
            } else {
                return Ok(Converted::Missing);
            };
        let hierarchy = match &hierarchy_name {
            Some(hierarchy) => {
                let own = file
                    .hierarchies
                    .iter()
                    .find(|candidate| &candidate.name == hierarchy)
                    .cloned();
                match own {
                    Some(own) => Some(own),
                    None => assets.file(hierarchy)?.and_then(|other| {
                        other
                            .hierarchies
                            .iter()
                            .find(|candidate| &candidate.name == hierarchy)
                            .cloned()
                    }),
                }
                .map(Some)
                .ok_or_else(|| error(ModelError::NoHierarchy(hierarchy.clone())))?
            }
            None => None,
        };
        let mut builder = ModelBuilder {
            name: name.clone(),
            glb: GlbBuilder::new(),
            keys: Vec::new(),
            materials: Vec::new(),
            missing_textures: BTreeSet::new(),
        };
        let parts = match &hierarchy {
            Some(hierarchy) => builder
                .hlod(hierarchy, &sub_objects, &meshes, assets)
                .map_err(error)?,
            None => builder
                .mesh_alone(
                    meshes[0]
                        .as_ref()
                        .expect("a model of one mesh is that mesh"),
                    assets,
                )
                .map_err(error)?,
        };
        Ok(Converted::Model(Box::new(ModelImport {
            glb: builder.glb,
            materials: builder.materials,
            parts,
            missing_textures: builder.missing_textures,
        })))
    }
}

/// A model as it is built: its glTF, its materials by their keys, the files they need, and the
/// textures no archive holds.
#[derive(Debug)]
struct ModelBuilder {
    name: String,
    glb: GlbBuilder,
    keys: Vec<MaterialKey>,
    materials: Vec<NamedMaterial>,
    missing_textures: BTreeSet<String>,
}

impl ModelBuilder {
    /// An HLOD: each pivot a node in its parent's, and each sub-object a node of its mesh on its
    /// pivot's; a skin's at the scene's root, its vertices already placed.
    fn hlod(
        &mut self,
        hierarchy: &Hierarchy,
        sub_objects: &[SubObject],
        meshes: &[Option<W3dMesh>],
        assets: &ModelAssets<'_>,
    ) -> Result<ModelParts, ModelError> {
        if sub_objects
            .iter()
            .any(|sub| sub.pivot >= hierarchy.pivots.len())
        {
            return Err(ModelError::Pivot);
        }
        let poses = Pose::of_each(&hierarchy.pivots);
        for pivot in &hierarchy.pivots {
            let pose = Pose::of(pivot);
            self.glb.document.nodes.push(Node {
                name: pivot.name.clone(),
                translation: Some(Pose::y_up(pose.translation)),
                rotation: Some(Pose::y_up_rotation(pose.rotation)),
                ..Node::default()
            });
        }
        for (at, pivot) in hierarchy.pivots.iter().enumerate() {
            match pivot.parent {
                Some(parent) => self.glb.document.nodes[parent].children.push(at),
                None => self.glb.document.scenes[0].nodes.push(at),
            }
        }
        let mut parts = ModelParts {
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
                parts.sub_objects.push(Part {
                    name: sub.name.clone(),
                    pivot: sub.pivot,
                    drawn: false,
                    hidden: true,
                });
                continue;
            };
            let mesh_index = self.mesh(mesh, Some(&poses), assets)?;
            let node = self.glb.document.nodes.len();
            self.glb.document.nodes.push(Node {
                name: sub.name.clone(),
                mesh: Some(mesh_index),
                ..Node::default()
            });
            if mesh.bones.is_some() {
                self.glb.document.scenes[0].nodes.push(node);
            } else {
                self.glb.document.nodes[sub.pivot].children.push(node);
            }
            parts.sub_objects.push(Part {
                name: sub.name.clone(),
                pivot: sub.pivot,
                drawn: true,
                hidden: mesh.hidden(),
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
            return Err(ModelError::NodeName(twice.name.clone()));
        }
        Ok(parts)
    }

    /// A mesh alone: one node of it, at the scene's root.
    fn mesh_alone(
        &mut self,
        mesh: &W3dMesh,
        assets: &ModelAssets<'_>,
    ) -> Result<ModelParts, ModelError> {
        if mesh.bones.is_some() {
            return Err(ModelError::SkinAlone);
        }
        let mesh_index = self.mesh(mesh, None, assets)?;
        self.glb.document.nodes.push(Node {
            name: mesh.name.clone(),
            mesh: Some(mesh_index),
            ..Node::default()
        });
        self.glb.document.scenes[0].nodes.push(0);
        Ok(ModelParts {
            sub_objects: vec![Part {
                name: mesh.name.clone(),
                pivot: 0,
                drawn: true,
                hidden: mesh.hidden(),
            }],
            pivots: Vec::new(),
        })
    }

    /// A glTF mesh of `mesh`: its vertices on glTF's axes, a skin's placed by each one's bone of
    /// `poses`; for each pass, one primitive for each texture, shader and vertex material its
    /// triangles draw, and for its detail stage one for each texture and shader that mixes it in,
    /// each a layer over what is drawn before it, all of the same vertices (design 12, D13).
    fn mesh(
        &mut self,
        mesh: &W3dMesh,
        poses: Option<&[Pose]>,
        assets: &ModelAssets<'_>,
    ) -> Result<usize, ModelError> {
        // A glTF mesh needs a primitive; none of the shipped meshes a model draws lacks one.
        if mesh.triangles.is_empty() {
            return Err(ModelError::Empty(mesh.name.clone()));
        }
        let pose_of = |vertex: usize| -> Result<Pose, ModelError> {
            match (&mesh.bones, poses) {
                (Some(bones), Some(poses)) => poses
                    .get(usize::from(bones[vertex]))
                    .copied()
                    .ok_or(ModelError::Pivot),
                _ => Ok(Pose::IDENTITY),
            }
        };
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut normals = Vec::with_capacity(mesh.normals.len());
        for (vertex, (position, normal)) in mesh.positions.iter().zip(&mesh.normals).enumerate() {
            let pose = pose_of(vertex)?;
            positions.push(Pose::y_up(pose.place(*position)));
            normals.push(Pose::y_up(pose.rotate(*normal)));
        }
        ModelBuilder::fill_zero_normals(&positions, &mesh.triangles, &mut normals);
        let position = self.glb.positions(&positions);
        let normal = self.glb.vectors(&normals);
        let attributes = |texcoord, color| Attributes {
            position,
            normal,
            texcoord,
            color,
        };
        let mut primitives = Vec::new();
        // Each pass, and its detail stage, draws over what is drawn before it, one depth bias up.
        let mut depth_bias = 0;
        for pass in &mesh.passes {
            let stage = pass.stage.as_ref();
            let lit =
                (0..mesh.triangles.len()).any(|at| !mesh.shader(pass.shaders.get(at)).unlit());
            if pass.colors.is_some() && lit && !ModelBuilder::colors_are_diffuse(mesh, pass) {
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
                let key = MaterialKey {
                    surface: self.surface(mesh, texture, assets),
                    source: Source::Pass {
                        shader,
                        material: *mesh.material(material),
                    },
                    two_sided: mesh.two_sided(),
                    depth_bias,
                };
                let material = self.material(key)?;
                let indices = self.glb.indices(&indices);
                let color = color.filter(|_| !shader.unlit());
                primitives.push(Primitive {
                    attributes: attributes(texcoord, color),
                    indices,
                    material,
                });
            }
            depth_bias += 1;
            let Some(detail) = &pass.detail else {
                continue;
            };
            let mut groups: BTreeMap<(Option<u32>, u32), Vec<u32>> = BTreeMap::new();
            for (at, triangle) in mesh.triangles.iter().enumerate() {
                let shader = pass.shaders.get(at);
                let blend = mesh.shader(shader).detail_blend();
                if blend.map_err(ModelError::DetailFunction)?.is_some() {
                    groups
                        .entry((detail.texture(at), shader))
                        .or_default()
                        .extend(triangle);
                }
            }
            if groups.is_empty() {
                continue;
            }
            let texcoord = Some(self.coordinates(detail, mesh.positions.len()));
            for ((texture, shader), indices) in groups {
                let blend = mesh.shader(shader).detail_blend();
                let blend = blend
                    .ok()
                    .flatten()
                    .expect("a group's shader mixes its detail");
                let key = MaterialKey {
                    surface: self.surface(mesh, texture, assets),
                    source: Source::Detail { blend },
                    two_sided: mesh.two_sided(),
                    depth_bias,
                };
                let material = self.material(key)?;
                let indices = self.glb.indices(&indices);
                primitives.push(Primitive {
                    attributes: attributes(texcoord, None),
                    indices,
                    material,
                });
            }
            depth_bias += 1;
        }
        self.glb.document.meshes.push(Mesh {
            name: mesh.name.clone(),
            primitives,
        });
        Ok(self.glb.document.meshes.len() - 1)
    }

    /// Whether the game's vertex colors are its diffuse color, as `MeshMatDescClass` makes them
    /// when its vertex materials use diffuse light, and ambient or not, but no emissive light:
    /// the colors times the diffuse color and opacity, which glTF's colors times its base color
    /// are. With emissive light, the game moves them to another channel. A mesh whose shaders
    /// draw their textures alone uses no vertex colors, so it needs none of this.
    fn colors_are_diffuse(mesh: &W3dMesh, pass: &Pass) -> bool {
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
    fn coordinates(&mut self, stage: &Stage, vertices: usize) -> usize {
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

    /// What a triangle of the texture `id` of `mesh` is drawn with: nothing for none, the
    /// archives' file of its name, or `MissingTexture` for a name no archive holds, which the
    /// report names.
    fn surface(&mut self, mesh: &W3dMesh, id: Option<u32>, assets: &ModelAssets<'_>) -> Surface {
        let Some(id) = id else {
            return Surface::Untextured;
        };
        let texture = &mesh.textures[usize::try_from(id).expect("a u32 fits usize")];
        if let Some(path) = assets.texture(&texture.name) {
            Surface::Texture {
                path,
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
    fn fill_zero_normals(positions: &[[f32; 3]], triangles: &[[u32; 3]], normals: &mut [[f32; 3]]) {
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

    /// The glTF material of `key`, the model's `<name>_<index>`, made at its first use; a material
    /// file beside it if its shader is more than glTF's material says. It is rough, not metallic,
    /// and of no specular, as no shader of the game turns its specular on; unlit, white and of no
    /// emissive light, when its shader draws its texture alone; and of `MissingTexture`'s color
    /// times its own in place of a texture no archive holds (design 12, D13).
    fn material(&mut self, key: MaterialKey) -> Result<usize, ModelError> {
        if let Some(at) = self.keys.iter().position(|known| *known == key) {
            return Ok(at);
        }
        let (blend, depth_write, colors) = match key.source {
            Source::Pass { shader, material } => {
                let blend = shader.blend().ok_or(ModelError::Shader {
                    src: shader.src_blend,
                    dest: shader.dest_blend,
                })?;
                (
                    blend,
                    shader.depth_write,
                    (!shader.unlit()).then_some(material),
                )
            }
            Source::Detail { blend } => (blend, false, None),
        };
        let name = format!("{}_{}", self.name, self.keys.len());
        let unlit = colors.is_none();
        let (lit_color, emissive) = match colors {
            None => ([1.0; 4], [0.0; 3]),
            Some(material) => {
                let [r, g, b] = material.diffuse.map(ModelBuilder::linear);
                let emissive = material.emissive.map(ModelBuilder::linear);
                ([r, g, b, material.opacity], emissive)
            }
        };
        let base_color = match key.surface {
            Surface::Missing => [0, 1, 2, 3].map(|at| lit_color[at] * MISSING[at]),
            Surface::Untextured | Surface::Texture { .. } => lit_color,
        };
        let texture = match &key.surface {
            Surface::Texture { path, flags } => Some(MaterialTexture {
                path: path.texture_path(),
                clamp_u: flags & CLAMP_U != 0,
                clamp_v: flags & CLAMP_V != 0,
            }),
            Surface::Untextured | Surface::Missing => None,
        };
        let gltf_texture = texture.as_ref().map(|texture| {
            let wrap = |clamp: bool| if clamp { CLAMP_TO_EDGE } else { REPEAT };
            let uri = format!(
                "../{}",
                ModelBuilder::uri(texture.path.as_str().trim_start_matches("client/"))
            );
            TextureRef {
                index: self
                    .glb
                    .texture(uri, wrap(texture.clamp_u), wrap(texture.clamp_v)),
            }
        });
        let (alpha_mode, alpha_cutoff, gltf_depth_write) = match blend {
            Blend::Opaque => ("OPAQUE", None, true),
            Blend::Mask { cutoff } => ("MASK", Some(cutoff), true),
            _ => ("BLEND", None, false),
        };
        let at = self.glb.material(Material {
            name: name.clone(),
            pbr: Pbr {
                base_color,
                texture: gltf_texture,
                metallic: METALLIC,
                roughness: ROUGHNESS,
            },
            emissive,
            alpha_mode,
            alpha_cutoff,
            double_sided: key.two_sided,
            extensions: MaterialExtensions {
                specular: Specular { factor: SPECULAR },
                unlit: unlit.then_some(Unlit {}),
            },
        });
        let says_all = matches!(blend, Blend::Opaque | Blend::Mask { .. } | Blend::Alpha)
            && gltf_depth_write == depth_write
            && key.depth_bias == 0;
        if !says_all {
            self.materials.push(NamedMaterial {
                name,
                file: MaterialFile {
                    base_color,
                    texture,
                    emissive,
                    metallic: METALLIC,
                    roughness: ROUGHNESS,
                    specular: SPECULAR,
                    unlit,
                    blend,
                    depth_write,
                    depth_bias: key.depth_bias,
                    double_sided: key.two_sided,
                },
            });
        }
        self.keys.push(key);
        debug_assert_eq!(at, self.keys.len() - 1, "a key is its material's index");
        Ok(at)
    }

    /// An sRGB code as glTF's linear factor: its light, at most 2¹⁶ steps of 2⁻¹⁶.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a light is at most 2¹⁶, exact in f32"
    )]
    fn linear(code: u8) -> f32 {
        Srgb::light(code) as f32 / 65_536.0
    }

    /// `path` as a relative URI: each byte outside RFC 3986's unreserved set and `/`
    /// percent-encoded.
    fn uri(path: &str) -> String {
        let mut uri = String::with_capacity(path.len());
        for byte in path.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
                uri.push(char::from(byte));
            } else {
                write!(uri, "%{byte:02X}").expect("a String takes any text");
            }
        }
        uri
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::zero_hour::hierarchy::{Hierarchy, Pivot};
    use crate::zero_hour::hlod::{Hlod, SubObject};
    use crate::zero_hour::w3d_file::internals::{box_chunk, emitter_chunk};
    use crate::zero_hour::w3d_mesh::internals::SKIN_GEOMETRY;
    use crate::zero_hour::w3d_mesh::{
        Ids, Pass, Shader, Stage, TWO_SIDED, Texture, VertexMaterial, W3dMesh,
    };

    /// `Tank.w3d`: the HLOD `TANK` on its hierarchy of a root, a turret a half turn about `z` at
    /// `(1, 2, 3)`, and a flash bone at `(0, 1, 0)` on the turret. Its sub-objects: a hull of
    /// vertex colors, coordinates of `NaN` and infinity at one vertex, and two triangles, one of a
    /// clamped texture `Rock.tga` and one of none; on
    /// the turret a triangle with zero normals and a vertex no triangle uses; on the flash bone an
    /// additive two-sided triangle of a texture no archive holds, `Gone.tga`, and no coordinates;
    /// a box; a name of no render object; and a skin on the turret.
    pub(crate) fn tank() -> Vec<u8> {
        let pivot = |name: &str, parent, translation, rotation| Pivot {
            name: name.to_owned(),
            parent,
            translation,
            rotation,
        };
        let identity = [0.0, 0.0, 0.0, 1.0];
        let hierarchy = Hierarchy {
            name: "TANK".to_owned(),
            pivots: vec![
                pivot("ROOTTRANSFORM", None, [0.0; 3], identity),
                pivot("TURRET", Some(0), [1.0, 2.0, 3.0], [0.0, 0.0, 1.0, 0.0]),
                pivot("MUZZLEFX01", Some(1), [0.0, 1.0, 0.0], identity),
            ],
        };
        let [hull, turret, flash, skin] = tank_meshes();
        let sub = |pivot, name: &str| SubObject {
            pivot,
            name: name.to_owned(),
        };
        let hlod = Hlod {
            name: "TANK".to_owned(),
            hierarchy: "TANK".to_owned(),
            sub_objects: vec![
                sub(0, "TANK.HULL"),
                sub(1, "TANK.TURRET"),
                sub(2, "TANK.FLASH"),
                sub(0, "TANK.PICKBOX"),
                sub(0, "TANK.GHOST"),
                sub(0, "TANK.SKIN"),
            ],
        };
        [
            hull.chunk(),
            turret.chunk(),
            flash.chunk(),
            skin.chunk(),
            box_chunk("TANK.PICKBOX"),
            hierarchy.chunk(),
            hlod.chunk_of(1),
        ]
        .concat()
    }

    /// The tank's meshes: its hull, its turret, its flash and its skin.
    fn tank_meshes() -> [W3dMesh; 4] {
        let hull = W3dMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            triangles: vec![[0, 1, 2], [2, 1, 3]],
            shaders: vec![Shader::DEFAULT],
            materials: vec![VertexMaterial {
                diffuse: [255, 128, 0],
                ..VertexMaterial::DEFAULT
            }],
            textures: vec![Texture {
                name: "Rock.tga".to_owned(),
                flags: 0x8,
            }],
            passes: vec![Pass {
                materials: Ids::One(0),
                shaders: Ids::One(0),
                colors: Some(vec![[10, 20, 30, 40]; 4]),
                stage: Some(Stage {
                    textures: Ids::Each(vec![0, Stage::NONE]),
                    coordinates: vec![
                        [0.0, 0.0],
                        [1.0, 0.0],
                        [0.0, 1.0],
                        [f32::NAN, f32::INFINITY],
                    ],
                }),
                detail: None,
            }],
            ..W3dMesh::named("TANK.HULL")
        };
        let turret = W3dMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [5.0, 5.0, 5.0],
            ],
            normals: vec![[0.0; 3], [0.0, -1.0, 0.0], [0.0, -1.0, 0.0], [0.0; 3]],
            triangles: vec![[0, 1, 2]],
            ..W3dMesh::named("TANK.TURRET")
        };
        let flash = W3dMesh {
            flags: TWO_SIDED,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            triangles: vec![[0, 1, 2]],
            shaders: vec![Shader {
                depth_write: false,
                dest_blend: 1,
                ..Shader::DEFAULT
            }],
            materials: vec![VertexMaterial {
                emissive: [255, 0, 0],
                ..VertexMaterial::DEFAULT
            }],
            textures: vec![Texture {
                name: "Gone.tga".to_owned(),
                flags: 0,
            }],
            passes: vec![Pass {
                materials: Ids::One(0),
                shaders: Ids::One(0),
                colors: None,
                stage: Some(Stage {
                    textures: Ids::One(0),
                    coordinates: Vec::new(),
                }),
                detail: None,
            }],
            ..W3dMesh::named("TANK.FLASH")
        };
        let skin = W3dMesh {
            flags: SKIN_GEOMETRY,
            positions: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            bones: Some(vec![1; 3]),
            triangles: vec![[0, 1, 2]],
            ..W3dMesh::named("TANK.SKIN")
        };
        [hull, turret, flash, skin]
    }

    /// `Rock01.w3d`: one mesh of an empty container, a triangle.
    pub(crate) fn rock() -> Vec<u8> {
        W3dMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            triangles: vec![[0, 1, 2]],
            ..W3dMesh::named("ROCK01")
        }
        .chunk()
    }

    /// `Prop.w3d`, a box alone, and `Spray.w3d`, an emitter.
    pub(crate) fn nothing_drawn() -> [Vec<u8>; 2] {
        [box_chunk("PROP"), emitter_chunk("SPRAY")]
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Json;
    use campfire_store::internals::Scratch;
    use campfire_values::PackagePath;

    use super::*;
    use crate::gltf::gltf_document::GltfDocument;
    use crate::zero_hour::hierarchy::Pivot;
    use crate::zero_hour::install::Install;
    use crate::zero_hour::install::internals::fixture;
    use crate::zero_hour::w3d_mesh::internals::GRADIENT_MODULATE;
    use crate::zero_hour::w3d_mesh::{Ids, Texture};

    fn convert(name: &str, assets: &mut ModelAssets<'_>) -> ModelImport {
        match ModelImport::convert(name, assets).unwrap() {
            Converted::Model(model) => *model,
            other => panic!("{name} converts to {other:?}"),
        }
    }

    /// Each material's name, base color, texture, alpha mode, two-sidedness and emissive color.
    type MaterialRow<'a> = (&'a str, [f32; 4], Option<usize>, &'a str, bool, [f32; 3]);

    /// Each node's name, children, translation, rotation and mesh.
    type NodeRow<'a> = (
        &'a str,
        Vec<usize>,
        Option<[f32; 3]>,
        Option<[f32; 4]>,
        Option<usize>,
    );

    /// The fixture's tank, converted.
    fn tank() -> ModelImport {
        let scratch = Scratch::new();
        fixture(&scratch);
        let install = Install::open(&scratch.path("zh")).unwrap();
        let mut assets = ModelAssets::new(&install).unwrap();
        convert("Tank", &mut assets)
    }

    #[test]
    fn an_hlod_becomes_nodes_on_its_pivots_with_meshes_on_glb_axes() {
        let tank = tank();
        let document = &tank.glb.document;

        // The pivots, each `(x, y, z)` as `(x, z, −y)`: the turret at (1, 3, −2), its half turn
        // about W3D's `z` one about glTF's `y`; the flash bone at (0, 0, −1). Then a node for
        // each sub-object that is a mesh, on its pivot's node; the skin's at the root. The box has
        // none, and the name of no render object is dropped.
        let nodes: Vec<NodeRow<'_>> = document
            .nodes
            .iter()
            .map(|node| {
                (
                    node.name.as_str(),
                    node.children.clone(),
                    node.translation,
                    node.rotation,
                    node.mesh,
                )
            })
            .collect();
        let identity = Some([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            nodes,
            [
                ("roottransform", vec![1, 3], Some([0.0; 3]), identity, None),
                (
                    "turret",
                    vec![2, 4],
                    Some([1.0, 3.0, -2.0]),
                    Some([0.0, 1.0, 0.0, 0.0]),
                    None
                ),
                (
                    "muzzlefx01",
                    vec![5],
                    Some([0.0, 0.0, -1.0]),
                    identity,
                    None
                ),
                ("tank.hull", vec![], None, None, Some(0)),
                ("tank.turret", vec![], None, None, Some(1)),
                ("tank.flash", vec![], None, None, Some(2)),
                ("tank.skin", vec![], None, None, Some(3)),
            ]
        );
        assert_eq!(document.scenes[0].nodes, [0, 6]);
        let names: Vec<&str> = document
            .meshes
            .iter()
            .map(|mesh| mesh.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["tank.hull", "tank.turret", "tank.flash", "tank.skin"]
        );

        // The hull: its vertices on glTF's axes, `V` as `1 − V`; its untextured triangle first,
        // as no texture sorts before one, then its textured one.
        let glb = &tank.glb;
        let hull = &document.meshes[0].primitives;
        let attributes = &hull[0].attributes;
        assert_eq!(
            glb.floats_of(attributes.position),
            [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, -1.0]
        );
        assert_eq!(glb.floats_of(attributes.normal), [0.0, 1.0, 0.0].repeat(4));
        // The last vertex's `NaN` and infinite coordinates are 0.
        assert_eq!(
            glb.floats_of(attributes.texcoord.unwrap()),
            [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            glb.colors_of(attributes.color.unwrap()),
            [10, 20, 30, 40].repeat(4)
        );
        assert_eq!(hull.len(), 2);
        assert_eq!(glb.indices_of(hull[0].indices), [2, 1, 3]);
        assert_eq!(glb.indices_of(hull[1].indices), [0, 1, 2]);
        assert_eq!((hull[0].material, hull[1].material), (0, 1));

        // The turret's zero normals: the first vertex's from its triangle's corners (0, 0, 0),
        // (1, 0, 0) and (0, 1, 0), whose normal is (0, 0, 1); the vertex no triangle uses, up.
        // Its other normals, (0, −1, 0), are (0, 0, 1) on glTF's axes.
        let turret = &document.meshes[1].primitives[0];
        assert_eq!(
            glb.floats_of(turret.attributes.normal),
            [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(turret.attributes.texcoord, None);

        // The flash's stage of no coordinates samples D3D's (0, 0), which is glTF's, at each
        // vertex; its texture is in no archive, so its triangle is untextured.
        let flash = &document.meshes[2].primitives[0];
        assert_eq!(glb.floats_of(flash.attributes.texcoord.unwrap()), [0.0; 6]);
        assert_eq!(flash.attributes.color, None);

        // The skin's vertices placed by the turret's pose into the model's frame: the half turn
        // takes (x, y, z) to (−x, −y, z), then the move by (1, 2, 3): (1, 0, 0) to (0, 2, 3),
        // (0, 1, 0) to (1, 1, 3), (0, 0, 1) to (1, 2, 4); on glTF's axes (0, 3, −2), (1, 3, −1)
        // and (1, 4, −2). Its normals turn with it, (0, 0, 1) staying up.
        let skin = &document.meshes[3].primitives[0];
        assert_eq!(
            glb.floats_of(skin.attributes.position),
            [0.0, 3.0, -2.0, 1.0, 3.0, -1.0, 1.0, 4.0, -2.0]
        );
        assert_eq!(
            glb.floats_of(skin.attributes.normal),
            [0.0, 1.0, 0.0].repeat(3)
        );
        // The turret and the skin share the default material, white and opaque.
        assert_eq!((turret.material, skin.material, flash.material), (2, 2, 3));
    }

    #[test]
    fn an_hlod_s_materials_are_its_shaders_and_its_parts_what_its_look_reads() {
        let tank = tank();
        let document = &tank.glb.document;
        // The materials, each of the model's name and its index: the hull's diffuse (255, 128,
        // 0) as linear light; its texture `Rock.tga` is the archives' `Rock.dds`, clamped in
        // `u`; the flash adds, which glTF's material cannot say, so it has a material file, and
        // its texture `Gone.tga` is in no archive, so its white is `MissingTexture`'s
        // `0x7FFF00FF`: red and blue 1, green 0, alpha 127 / 255.
        let half = f32::from(u16::try_from(Srgb::light(128)).unwrap()) / 65_536.0;
        let rows: Vec<MaterialRow<'_>> = document
            .materials
            .iter()
            .map(|material| {
                let texture = material.pbr.texture.as_ref().map(|texture| texture.index);
                (
                    material.name.as_str(),
                    material.pbr.base_color,
                    texture,
                    material.alpha_mode,
                    material.double_sided,
                    material.emissive,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "tank_0",
                    [1.0, half, 0.0, 1.0],
                    None,
                    "OPAQUE",
                    false,
                    [0.0; 3]
                ),
                (
                    "tank_1",
                    [1.0, half, 0.0, 1.0],
                    Some(0),
                    "OPAQUE",
                    false,
                    [0.0; 3]
                ),
                ("tank_2", [1.0; 4], None, "OPAQUE", false, [0.0; 3]),
                (
                    "tank_3",
                    [1.0, 0.0, 1.0, 127.0 / 255.0],
                    None,
                    "BLEND",
                    true,
                    [1.0, 0.0, 0.0]
                ),
            ]
        );
        // Each is rough, not metallic and of no specular, and lit, as each shader modulates.
        for material in &document.materials {
            let pbr = &material.pbr;
            let extensions = &material.extensions;
            assert_eq!((pbr.metallic, pbr.roughness), (0.0, 1.0));
            assert_eq!(extensions.specular.factor, 0.0);
            assert!(extensions.unlit.is_none());
        }
        assert_eq!(document.extensions_used, ["KHR_materials_specular"]);
        assert_eq!(document.images[0].uri, "../textures/art/textures/rock.ktx2");
        // A byte outside RFC 3986's unreserved set and `/` is percent-encoded, a UTF-8 one by byte.
        assert_eq!(ModelBuilder::uri("a b/ü~.ktx2"), "a%20b/%C3%BC~.ktx2");
        assert_eq!(
            (document.samplers[0].wrap_s, document.samplers[0].wrap_t),
            (CLAMP_TO_EDGE, REPEAT)
        );
        assert_eq!(
            tank.missing_textures,
            BTreeSet::from(["gone.tga".to_owned()])
        );
        assert_eq!(tank.materials.len(), 1);
        assert_eq!(tank.materials[0].name, "tank_3");
        assert_eq!(
            tank.materials[0].file,
            MaterialFile {
                base_color: [1.0, 0.0, 1.0, 127.0 / 255.0],
                texture: None,
                emissive: [1.0, 0.0, 0.0],
                metallic: 0.0,
                roughness: 1.0,
                specular: 0.0,
                unlit: false,
                blend: Blend::Add,
                depth_write: false,
                depth_bias: 0,
                double_sided: true,
            }
        );

        // What the default look reads: each sub-object the HLOD keeps, the box with no node.
        let part = |name: &str, pivot, drawn, hidden| Part {
            name: name.to_owned(),
            pivot,
            drawn,
            hidden,
        };
        assert_eq!(
            tank.parts.sub_objects,
            [
                part("tank.hull", 0, true, false),
                part("tank.turret", 1, true, false),
                part("tank.flash", 2, true, false),
                part("tank.pickbox", 0, false, true),
                part("tank.skin", 0, true, false),
            ]
        );
        let pivots: Vec<(&str, Option<usize>)> = tank
            .parts
            .pivots
            .iter()
            .map(|pivot| (pivot.name.as_str(), pivot.parent))
            .collect();
        assert_eq!(
            pivots,
            [
                ("roottransform", None),
                ("turret", Some(0)),
                ("muzzlefx01", Some(1))
            ]
        );
    }

    #[test]
    fn a_name_is_an_hlod_a_mesh_alone_or_a_render_object_that_draws_nothing() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let install = Install::open(&scratch.path("zh")).unwrap();
        let mut assets = ModelAssets::new(&install).unwrap();
        // `File.Mesh` is a mesh alone, at the scene's root, and its materials are named by it.
        let hull = convert("TANK.HULL", &mut assets);
        let document = &hull.glb.document;
        assert_eq!(document.nodes.len(), 1);
        assert_eq!(
            (document.nodes[0].name.as_str(), document.nodes[0].mesh),
            ("tank.hull", Some(0))
        );
        assert_eq!(document.scenes[0].nodes, [0]);
        assert_eq!(document.materials[0].name, "tank.hull_0");
        // A mesh of an empty container is named alone; its file is the language folder's, as
        // the other `Rock01.w3d` is no W3D file.
        let rock = convert("rock01", &mut assets);
        assert_eq!(rock.glb.document.nodes[0].name, "rock01");
        // `NULL` and a box alone draw nothing; an emitter is not converted; a name of no file
        // and a name its file lacks are missing.
        let outcome =
            |name: &str, assets: &mut ModelAssets<'_>| match ModelImport::convert(name, assets)
                .unwrap()
            {
                Converted::Model(_) => "model",
                Converted::Nothing => "nothing",
                Converted::Emitter => "emitter",
                Converted::Missing => "missing",
            };
        assert_eq!(outcome("Null", &mut assets), "nothing");
        assert_eq!(outcome("prop", &mut assets), "nothing");
        assert_eq!(outcome("spray", &mut assets), "emitter");
        assert_eq!(outcome("absent", &mut assets), "missing");
        assert_eq!(outcome("tank.ghost", &mut assets), "missing");
    }

    #[test]
    fn a_zero_normal_takes_its_triangles_normals_by_area_or_up() {
        // Vertex 0 lies on a triangle of area 2 facing `z` and one of area 1 facing `x`. Vertex 4
        // lies on a triangle of no area: up.
        let positions = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 1.0],
            [3.0, 3.0, 3.0],
        ];
        let triangles = [[0, 1, 2], [0, 2, 3], [4, 4, 4]];
        let mut normals = [
            [0.0; 3],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0; 3],
        ];
        ModelBuilder::fill_zero_normals(&positions, &triangles, &mut normals);
        // Each cross product is twice its triangle's area: (2, 0, 0) × (0, 2, 0) = (0, 0, 4) and
        // (0, 2, 0) × (0, 0, 1) = (2, 0, 0); their sum, (2, 0, 4), made unit by √20.
        let length = 20.0_f32.sqrt();
        assert_eq!(normals[0], [2.0 / length, 0.0, 4.0 / length]);
        assert_eq!(
            normals[1..4],
            [[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]
        );
        assert_eq!(normals[4], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn vertex_colors_convert_only_as_the_diffuse_color() {
        // As `MeshMatDescClass` reads them: diffuse light, with ambient or not, takes the colors;
        // with emissive light, or with none of diffuse, they go elsewhere.
        let material = |ambient, diffuse, emissive| VertexMaterial {
            ambient,
            diffuse,
            emissive,
            opacity: 1.0,
        };
        let (on, off) = ([1, 0, 0], [0; 3]);
        let cases = [
            (vec![material(off, on, off)], true),
            (vec![material(on, on, off)], true),
            (vec![material(on, off, off)], false),
            (vec![material(off, on, on)], false),
            (vec![material(off, off, off)], false),
            // Each vertex's material counts: one emits, so the mesh's colors are not diffuse.
            (vec![material(off, on, off), material(off, off, on)], false),
        ];
        for (materials, diffuse) in cases {
            let ids = if materials.len() == 1 {
                Ids::One(0)
            } else {
                Ids::Each(vec![0, 1])
            };
            let mut mesh = W3dMesh {
                positions: vec![[0.0; 3]; 2],
                materials,
                ..W3dMesh::named("mesh")
            };
            mesh.passes[0].materials = ids;
            assert_eq!(
                ModelBuilder::colors_are_diffuse(&mesh, &mesh.passes[0]),
                diffuse,
                "{:?}",
                mesh.materials
            );
        }
    }

    #[test]
    fn a_shader_of_no_primary_gradient_draws_its_texture_alone_unlit() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let install = Install::open(&scratch.path("zh")).unwrap();
        let assets = ModelAssets::new(&install).unwrap();
        // Two triangles of vertex colors, one of `Rock.tga`, which the archives hold, and one of
        // `Gone.tga`, which they do not; a diffuse of (255, 128, 0), half opaque, and `emissive`
        // light, which vertex colors allow only where no lit shader draws them.
        let mesh = |primary_gradient, emissive| W3dMesh {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0; 3]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            triangles: vec![[0, 1, 2], [2, 1, 3]],
            shaders: vec![Shader {
                primary_gradient,
                ..Shader::DEFAULT
            }],
            materials: vec![VertexMaterial {
                diffuse: [255, 128, 0],
                emissive,
                opacity: 0.5,
                ..VertexMaterial::DEFAULT
            }],
            textures: ["Rock.tga", "Gone.tga"]
                .map(|name| Texture {
                    name: name.to_owned(),
                    flags: 0,
                })
                .into(),
            passes: vec![Pass {
                materials: Ids::One(0),
                shaders: Ids::One(0),
                colors: Some(vec![[10, 20, 30, 40]; 4]),
                stage: Some(Stage {
                    textures: Ids::Each(vec![0, 1]),
                    coordinates: vec![[0.0; 2]; 4],
                }),
                detail: None,
            }],
            ..W3dMesh::named("decal")
        };
        let build = |primary_gradient, emissive| {
            let mut builder = ModelBuilder {
                name: "decal".to_owned(),
                glb: GlbBuilder::new(),
                keys: Vec::new(),
                materials: Vec::new(),
                missing_textures: BTreeSet::new(),
            };
            let mesh = mesh(primary_gradient, emissive);
            builder
                .mesh(&mesh, None, &assets)
                .map(|_| builder.glb.document)
        };
        // Each material's base color, whether it is textured, its emissive light and whether it
        // is unlit; whether each primitive has vertex colors.
        let rows = |document: &GltfDocument| -> Vec<([f32; 4], bool, [f32; 3], bool)> {
            document
                .materials
                .iter()
                .map(|material| {
                    (
                        material.pbr.base_color,
                        material.pbr.texture.is_some(),
                        material.emissive,
                        material.extensions.unlit.is_some(),
                    )
                })
                .collect()
        };
        let colored = |document: &GltfDocument| -> Vec<bool> {
            document.meshes[0]
                .primitives
                .iter()
                .map(|primitive| primitive.attributes.color.is_some())
                .collect()
        };
        // Lit, the texture times the diffuse color, half opaque; the missing texture's
        // `0x7FFF00FF` times them: (1, 0.216, 0, 0.5) × (1, 0, 1, 127 / 255).
        let half = f32::from(u16::try_from(Srgb::light(128)).unwrap()) / 65_536.0;
        let lit = build(GRADIENT_MODULATE, [0; 3]).unwrap();
        assert_eq!(
            rows(&lit),
            [
                ([1.0, half, 0.0, 0.5], true, [0.0; 3], false),
                ([1.0, 0.0, 0.0, 0.5 * 127.0 / 255.0], false, [0.0; 3], false),
            ]
        );
        assert_eq!(colored(&lit), [true, true]);
        assert_eq!(
            build(GRADIENT_MODULATE, [255, 0, 0]).unwrap_err(),
            ModelError::VertexColors
        );
        assert_eq!(lit.extensions_used, ["KHR_materials_specular"]);
        // Unlit, the texture alone: white, opaque, of no emissive light, red as it is here, or
        // vertex colors; the missing texture's color alone.
        let unlit = build(0, [255, 0, 0]).unwrap();
        assert_eq!(
            rows(&unlit),
            [
                ([1.0; 4], true, [0.0; 3], true),
                ([1.0, 0.0, 1.0, 127.0 / 255.0], false, [0.0; 3], true),
            ]
        );
        assert_eq!(colored(&unlit), [false, false]);
        assert_eq!(
            unlit.extensions_used,
            ["KHR_materials_specular", "KHR_materials_unlit"]
        );
        // As glTF's JSON writes them: a specular factor of 0, and the unlit extension an empty
        // object.
        let json = Json::write(&unlit);
        assert!(
            json.contains(r#""KHR_materials_specular":{"specularFactor":0.0}"#),
            "{json}"
        );
        assert!(json.contains(r#""KHR_materials_unlit":{}"#), "{json}");
    }

    #[test]
    fn each_pass_and_detail_stage_is_a_layer_over_the_one_before() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let install = Install::open(&scratch.path("zh")).unwrap();
        let assets = ModelAssets::new(&install).unwrap();
        let mut builder = ModelBuilder {
            name: "layers".to_owned(),
            glb: GlbBuilder::new(),
            keys: Vec::new(),
            materials: Vec::new(),
            missing_textures: BTreeSet::new(),
        };
        let stage = |coordinates: Vec<[f32; 2]>| Stage {
            textures: Ids::One(0),
            coordinates,
        };
        let pass = |shaders, colors, stage, detail| Pass {
            materials: Ids::One(0),
            shaders,
            colors,
            stage,
            detail,
        };
        // A glow of two triangles: its first pass opaque, of `Rock.tga`, by two shaders, whose
        // detail functions scale and add its detail stage; its second pass additive, writing
        // no depth. Each stage has its own coordinates.
        let opaque = |detail_color| Shader {
            detail_color,
            ..Shader::DEFAULT
        };
        let additive = Shader {
            dest_blend: 1,
            depth_write: false,
            ..Shader::DEFAULT
        };
        let glow_mesh = W3dMesh {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0; 3]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            triangles: vec![[0, 1, 2], [2, 1, 3]],
            // `DETAILCOLOR_SCALE` and `DETAILCOLOR_ADD`.
            shaders: vec![opaque(2), opaque(4), additive],
            textures: vec![Texture {
                name: "Rock.tga".to_owned(),
                flags: 0,
            }],
            passes: vec![
                pass(
                    Ids::Each(vec![0, 1]),
                    None,
                    Some(stage(vec![[0.0, 0.0]; 4])),
                    Some(stage(vec![[0.5, 0.25]; 4])),
                ),
                pass(Ids::One(2), None, Some(stage(vec![[2.0, 0.0]; 4])), None),
            ],
            ..W3dMesh::named("glow")
        };
        // A fade of one triangle: an opaque first pass of no texture, and a second by alpha,
        // writing no depth, of its own vertex colors and alphas.
        let blended = Shader {
            src_blend: 2,
            dest_blend: 5,
            depth_write: false,
            ..Shader::DEFAULT
        };
        let fade_mesh = W3dMesh {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            triangles: vec![[0, 1, 2]],
            shaders: vec![Shader::DEFAULT, blended],
            passes: vec![
                pass(Ids::One(0), None, None, None),
                pass(Ids::One(1), Some(vec![[255, 255, 255, 64]; 3]), None, None),
            ],
            ..W3dMesh::named("fade")
        };
        builder.mesh(&glow_mesh, None, &assets).unwrap();
        builder.mesh(&fade_mesh, None, &assets).unwrap();
        let document = &builder.glb.document;
        let [glow, fade] = [0, 1].map(|at| &document.meshes[at].primitives);
        // Each primitive of a mesh draws its one list of vertices.
        for primitives in [glow, fade] {
            let position = primitives[0].attributes.position;
            assert!(
                primitives
                    .iter()
                    .all(|primitive| primitive.attributes.position == position)
            );
        }
        // The glow: its first pass by shader, then its detail stage by shader, then its second
        // pass; each layer's coordinates its own stage's, `V` read as `1 − V`.
        let texcoords: Vec<Vec<f32>> = glow
            .iter()
            .map(|primitive| {
                builder
                    .glb
                    .floats_of(primitive.attributes.texcoord.unwrap())
            })
            .collect();
        let uv = |u: f32, v: f32| [u, v].repeat(4);
        assert_eq!(
            texcoords,
            [
                uv(0.0, 1.0),
                uv(0.0, 1.0),
                uv(0.5, 0.75),
                uv(0.5, 0.75),
                uv(2.0, 1.0)
            ]
        );
        let triangles: Vec<usize> = glow
            .iter()
            .map(|primitive| document.accessors[primitive.indices].count / 3)
            .collect();
        assert_eq!(triangles, [1, 1, 1, 1, 2]);
        assert!(
            glow.iter()
                .all(|primitive| primitive.attributes.color.is_none())
        );
        // Each layer's material has its file, of its blend and its depth bias over the one
        // under it: the detail stage's, white and unlit, multiplying for scale and adding for
        // add, 1 up; the second pass's, lit by its white vertex material, additive, 2 up. The
        // first pass's two say all in glTF.
        let file = |name: &str| {
            builder
                .materials
                .iter()
                .find(|material| material.name == name)
                .map(|material| &material.file)
        };
        let layer = |blend, unlit, depth_bias| MaterialFile {
            base_color: [1.0; 4],
            texture: Some(MaterialTexture {
                path: PackagePath::parse("client/textures/art/textures/rock.ktx2").unwrap(),
                clamp_u: false,
                clamp_v: false,
            }),
            emissive: [0.0; 3],
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
            unlit,
            blend,
            depth_write: false,
            depth_bias,
            double_sided: false,
        };
        let materials: Vec<usize> = glow.iter().map(|primitive| primitive.material).collect();
        assert_eq!(materials, [0, 1, 2, 3, 4]);
        assert_eq!((file("layers_0"), file("layers_1")), (None, None));
        assert_eq!(file("layers_2"), Some(&layer(Blend::Multiply, true, 1)));
        assert_eq!(file("layers_3"), Some(&layer(Blend::Add, true, 1)));
        assert_eq!(file("layers_4"), Some(&layer(Blend::Add, false, 2)));
        // The fade: its first pass of no colors; its second of its own, alpha 64 of 255, by
        // alpha, 1 up.
        assert_eq!(fade[0].attributes.color, None);
        let colors = builder.glb.colors_of(fade[1].attributes.color.unwrap());
        assert_eq!(colors, [255, 255, 255, 64].repeat(3));
        let alpha = MaterialFile {
            texture: None,
            ..layer(Blend::Alpha, false, 1)
        };
        assert_eq!(file("layers_6"), Some(&alpha));
        // A detail function no shipped shader uses is refused.
        let mut odd = glow_mesh;
        odd.shaders[0].detail_color = 3;
        assert_eq!(
            builder.mesh(&odd, None, &assets).unwrap_err(),
            ModelError::DetailFunction(3)
        );
    }

    #[test]
    fn an_hlod_of_a_pivot_its_hierarchy_lacks_or_of_two_nodes_of_one_name_is_refused() {
        let scratch = Scratch::new();
        fixture(&scratch);
        let install = Install::open(&scratch.path("zh")).unwrap();
        let assets = ModelAssets::new(&install).unwrap();
        let pivot = |name: &str, parent| Pivot {
            name: name.to_owned(),
            parent,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        let hierarchy = Hierarchy {
            name: "h".to_owned(),
            pivots: vec![pivot("root", None), pivot("h.part", Some(0))],
        };
        let mesh = W3dMesh {
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            triangles: vec![[0, 1, 2]],
            ..W3dMesh::named("h.part")
        };
        let build = |pivot, mesh: &W3dMesh| {
            let mut builder = ModelBuilder {
                name: "h".to_owned(),
                glb: GlbBuilder::new(),
                keys: Vec::new(),
                materials: Vec::new(),
                missing_textures: BTreeSet::new(),
            };
            let sub = SubObject {
                pivot,
                name: "h.part".to_owned(),
            };
            builder.hlod(&hierarchy, &[sub], &[Some(mesh.clone())], &assets)
        };
        assert_eq!(build(2, &mesh).unwrap_err(), ModelError::Pivot);
        let empty = W3dMesh {
            triangles: Vec::new(),
            ..mesh.clone()
        };
        assert_eq!(
            build(1, &empty).unwrap_err(),
            ModelError::Empty("h.part".to_owned())
        );
        // The pivot `h.part` and the sub-object `h.part` are two nodes of one name.
        assert_eq!(
            build(0, &mesh).unwrap_err(),
            ModelError::NodeName("h.part".to_owned())
        );
    }
}
