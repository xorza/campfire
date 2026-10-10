use campfire_package::Blend;

use crate::zero_hour::error::W3dError;
use crate::zero_hour::w3d_file::{Chunks, Fields};

/// A W3D mesh, what a model draws of it: its full name, its flags, its vertices, its triangles,
/// and its first material pass, as `MeshModelClass` reads them. Every float but a texture
/// coordinate is finite: 35 shipped meshes hold `NaN` ones.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct W3dMesh {
    /// `container.mesh`, or `mesh` alone if the container's name is empty.
    pub(crate) name: String,
    pub(crate) flags: u32,
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) normals: Vec<[f32; 3]>,
    /// For a skin, each vertex's bone.
    pub(crate) bones: Option<Vec<u16>>,
    /// Each triangle's vertices, counter-clockwise from its front.
    pub(crate) triangles: Vec<[u32; 3]>,
    pub(crate) shaders: Vec<Shader>,
    pub(crate) materials: Vec<VertexMaterial>,
    pub(crate) textures: Vec<Texture>,
    pub(crate) pass: Pass,
}

/// How a mesh's pixels meet what is drawn behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shader {
    pub(crate) depth_write: bool,
    pub(crate) dest_blend: u8,
    pub(crate) src_blend: u8,
    pub(crate) alpha_test: bool,
    pub(crate) texturing: bool,
}

/// A vertex material's colors: ambient, diffuse and emissive, each red, green and blue, and its
/// opacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VertexMaterial {
    pub(crate) ambient: [u8; 3],
    pub(crate) diffuse: [u8; 3],
    pub(crate) emissive: [u8; 3],
    pub(crate) opacity: f32,
}

/// A texture a mesh names, and its flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Texture {
    pub(crate) name: String,
    pub(crate) flags: u16,
}

/// A material pass: its vertex material, shader and color of each vertex or triangle, and its
/// first texture stage.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pass {
    /// By vertex.
    pub(crate) materials: Ids,
    /// By triangle.
    pub(crate) shaders: Ids,
    /// Each vertex's red, green, blue and alpha.
    pub(crate) colors: Option<Vec<[u8; 4]>>,
    pub(crate) stage: Option<Stage>,
}

/// A texture stage: its texture of each triangle, [`Stage::NONE`] for a triangle with none, and
/// each vertex's `u, v`, `v` as the file holds it; none if the file holds none, where the game
/// samples every vertex at `0, 0`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stage {
    pub(crate) textures: Ids,
    pub(crate) coordinates: Vec<[f32; 2]>,
}

/// An index into a list: one for all, or one for each vertex or triangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ids {
    One(u32),
    Each(Vec<u32>),
}

/// The flags a mesh's header holds.
pub(crate) const HIDDEN: u32 = 0x1000;
pub(crate) const TWO_SIDED: u32 = 0x2000;
const GEOMETRY: u32 = 0x00FF_0000;
const SKIN: u32 = 0x0002_0000;

const HEADER: u32 = 0x1F;
const VERTICES: u32 = 0x02;
const NORMALS: u32 = 0x03;
const INFLUENCES: u32 = 0x0E;
const TRIANGLES: u32 = 0x20;
const SHADERS: u32 = 0x29;
const VERTEX_MATERIALS: u32 = 0x2A;
const VERTEX_MATERIAL: u32 = 0x2B;
const VERTEX_MATERIAL_INFO: u32 = 0x2D;
const TEXTURES: u32 = 0x30;
const TEXTURE: u32 = 0x31;
const TEXTURE_NAME: u32 = 0x32;
const TEXTURE_INFO: u32 = 0x33;
const MATERIAL_PASS: u32 = 0x38;
const VERTEX_MATERIAL_IDS: u32 = 0x39;
const SHADER_IDS: u32 = 0x3A;
const COLORS: u32 = 0x3B;
const TEXTURE_STAGE: u32 = 0x48;
const TEXTURE_IDS: u32 = 0x49;
const STAGE_COORDINATES: u32 = 0x4A;

/// A name's bytes, `W3D_NAME_LEN`.
const NAME: usize = 16;

/// An alpha test's cutoff, 96 of 255, `D3DRS_ALPHAREF = 0x60` as the game sets it.
const CUTOFF: f32 = 96.0 / 255.0;

/// W3D's blend factors, `W3DSHADER_SRCBLENDFUNC_*` and `W3DSHADER_DESTBLENDFUNC_*`.
const ZERO: u8 = 0;
const ONE: u8 = 1;
const SRC_ALPHA: u8 = 2;
const SRC_COLOR: u8 = 2;
const ONE_MINUS_SRC_COLOR: u8 = 3;
const ONE_MINUS_SRC_ALPHA: u8 = 5;

impl W3dMesh {
    pub(crate) fn read(body: &[u8]) -> Result<W3dMesh, W3dError> {
        let mut header = None;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut bones = None;
        let mut triangles = Vec::new();
        let mut shaders = Vec::new();
        let mut materials = Vec::new();
        let mut textures = Vec::new();
        let mut pass = None;
        for chunk in Chunks::of(body) {
            let chunk = chunk?;
            let mut fields = Fields::of(chunk.body);
            match chunk.id {
                HEADER => {
                    fields.u32()?;
                    let flags = fields.u32()?;
                    let name = fields.name(NAME)?;
                    let container = fields.name(NAME)?;
                    let triangles = fields.u32()?;
                    let vertices = fields.u32()?;
                    header = Some((flags, name, container, triangles, vertices));
                }
                VERTICES => positions = W3dMesh::vectors(&mut fields)?,
                NORMALS => normals = W3dMesh::vectors(&mut fields)?,
                INFLUENCES => {
                    let records = fields.records::<8>()?;
                    bones = Some(
                        records
                            .iter()
                            .map(|record| u16::from_le_bytes([record[0], record[1]]))
                            .collect(),
                    );
                }
                TRIANGLES => triangles = W3dMesh::triangles(&mut fields)?,
                SHADERS => shaders = W3dMesh::shaders(&mut fields)?,
                VERTEX_MATERIALS => {
                    for material in Chunks::of(chunk.body) {
                        let material = material?;
                        if material.id != VERTEX_MATERIAL {
                            continue;
                        }
                        let info = Chunks::of(material.body)
                            .find(|inner| matches!(inner, Ok(inner) if inner.id == VERTEX_MATERIAL_INFO))
                            .ok_or(W3dError::Missing)??;
                        materials.push(W3dMesh::vertex_material(info.body)?);
                    }
                }
                TEXTURES => {
                    for texture in Chunks::of(chunk.body) {
                        let texture = texture?;
                        if texture.id == TEXTURE {
                            textures.push(W3dMesh::texture(texture.body)?);
                        }
                    }
                }
                // Only the first pass draws, as the slice takes it.
                MATERIAL_PASS if pass.is_none() => {
                    pass = Some(W3dMesh::pass(chunk.body)?);
                }
                _ => {}
            }
        }
        let (flags, name, container, triangle_count, vertex_count) =
            header.ok_or(W3dError::Missing)?;
        let mesh = W3dMesh {
            name: if container.is_empty() {
                name
            } else {
                format!("{container}.{name}")
            },
            flags,
            positions,
            normals,
            bones,
            triangles,
            shaders,
            materials,
            textures,
            // A mesh of no pass is drawn by no material; none ships.
            pass: pass.ok_or(W3dError::Missing)?,
        };
        mesh.check(triangle_count, vertex_count)?;
        Ok(mesh)
    }

    /// Whether the game draws it hidden until a state shows it.
    pub(crate) const fn hidden(&self) -> bool {
        self.flags & HIDDEN != 0
    }

    pub(crate) const fn two_sided(&self) -> bool {
        self.flags & TWO_SIDED != 0
    }

    /// The shader of `id`, which the read checked.
    pub(crate) fn shader(&self, id: u32) -> &Shader {
        &self.shaders[usize::try_from(id).expect("a u32 fits usize")]
    }

    /// The vertex material of `id`, which the read checked.
    pub(crate) fn material(&self, id: u32) -> &VertexMaterial {
        &self.materials[usize::try_from(id).expect("a u32 fits usize")]
    }

    /// Refuses a mesh whose lists are not its header's counts, whose indices pass their lists, or
    /// whose positions, normals or opacities are not all finite, which glTF requires.
    fn check(&self, triangles: u32, vertices: u32) -> Result<(), W3dError> {
        let vertices = usize::try_from(vertices).expect("a u32 fits usize");
        let triangles = usize::try_from(triangles).expect("a u32 fits usize");
        let geometry = self.flags & GEOMETRY;
        if geometry != 0 && geometry != SKIN {
            return Err(W3dError::Geometry(geometry));
        }
        let skinned = geometry == SKIN;
        let per_vertex = |len: usize| len == vertices;
        if !per_vertex(self.positions.len())
            || !per_vertex(self.normals.len())
            || self.triangles.len() != triangles
            || self
                .bones
                .as_ref()
                .is_some_and(|bones| !per_vertex(bones.len()))
            || self.bones.is_some() != skinned
        {
            return Err(W3dError::Counts);
        }
        let below = |index: u32, len: usize| usize::try_from(index).is_ok_and(|index| index < len);
        if self
            .triangles
            .iter()
            .flatten()
            .any(|&vertex| !below(vertex, vertices))
        {
            return Err(W3dError::Index);
        }
        let finite = |floats: &[f32]| floats.iter().all(|float| float.is_finite());
        if !finite(self.positions.as_flattened())
            || !finite(self.normals.as_flattened())
            || !self
                .materials
                .iter()
                .all(|material| material.opacity.is_finite())
        {
            return Err(W3dError::NotFinite);
        }
        let pass = &self.pass;
        let ids = |ids: &Ids, each: usize, len: usize| match ids {
            Ids::One(id) => below(*id, len),
            Ids::Each(ids) => ids.len() == each && ids.iter().all(|&id| below(id, len)),
        };
        let colors = pass
            .colors
            .as_ref()
            .is_none_or(|colors| per_vertex(colors.len()));
        let stage = pass.stage.as_ref().is_none_or(|stage| {
            let textures = match &stage.textures {
                Ids::One(id) => below(*id, self.textures.len()),
                Ids::Each(ids) => {
                    ids.len() == triangles
                        && ids
                            .iter()
                            .all(|&id| id == Stage::NONE || below(id, self.textures.len()))
                }
            };
            textures && (stage.coordinates.is_empty() || per_vertex(stage.coordinates.len()))
        });
        if !ids(&pass.materials, vertices, self.materials.len())
            || !ids(&pass.shaders, triangles, self.shaders.len())
            || !colors
            || !stage
        {
            return Err(W3dError::Index);
        }
        Ok(())
    }

    /// `W3dTriStruct`s: each triangle's three vertices, then what the importer does not read.
    fn triangles(fields: &mut Fields<'_>) -> Result<Vec<[u32; 3]>, W3dError> {
        Ok(fields
            .records::<32>()?
            .iter()
            .map(|record| {
                let index =
                    |at: usize| u32::from_le_bytes(record[at..at + 4].try_into().expect("4 bytes"));
                [index(0), index(4), index(8)]
            })
            .collect())
    }

    /// `W3dShaderStruct`s, each of 16 bytes.
    fn shaders(fields: &mut Fields<'_>) -> Result<Vec<Shader>, W3dError> {
        Ok(fields
            .records::<16>()?
            .iter()
            .map(|shader| Shader {
                depth_write: shader[1] != 0,
                dest_blend: shader[3],
                src_blend: shader[7],
                alpha_test: shader[12] != 0,
                texturing: shader[8] != 0,
            })
            .collect())
    }

    fn vectors(fields: &mut Fields<'_>) -> Result<Vec<[f32; 3]>, W3dError> {
        Ok(fields
            .records::<12>()?
            .iter()
            .map(|record| {
                let at =
                    |at: usize| f32::from_le_bytes(record[at..at + 4].try_into().expect("4 bytes"));
                [at(0), at(4), at(8)]
            })
            .collect())
    }

    /// `W3dVertexMaterialStruct`: flags, then ambient, diffuse, specular and emissive colors of
    /// four bytes each, then shininess, opacity and translucency.
    fn vertex_material(body: &[u8]) -> Result<VertexMaterial, W3dError> {
        let mut fields = Fields::of(body);
        fields.u32()?;
        let color = |fields: &mut Fields<'_>| -> Result<[u8; 3], W3dError> {
            let bytes = fields.take(4)?;
            Ok([bytes[0], bytes[1], bytes[2]])
        };
        let ambient = color(&mut fields)?;
        let diffuse = color(&mut fields)?;
        color(&mut fields)?;
        let emissive = color(&mut fields)?;
        fields.f32()?;
        let opacity = fields.f32()?;
        Ok(VertexMaterial {
            ambient,
            diffuse,
            emissive,
            opacity,
        })
    }

    fn texture(body: &[u8]) -> Result<Texture, W3dError> {
        let mut name = None;
        let mut flags = 0;
        for chunk in Chunks::of(body) {
            let chunk = chunk?;
            let mut fields = Fields::of(chunk.body);
            match chunk.id {
                TEXTURE_NAME => name = Some(fields.name(chunk.body.len())?),
                TEXTURE_INFO => flags = fields.u16()?,
                _ => {}
            }
        }
        Ok(Texture {
            name: name.ok_or(W3dError::Missing)?,
            flags,
        })
    }

    /// A pass, each of its lists from the first chunk that holds it: `MeshModelClass` reads a later
    /// one into the alternate material, which a draw's default look does not show.
    fn pass(body: &[u8]) -> Result<Pass, W3dError> {
        let mut materials = None;
        let mut shaders = None;
        let mut colors = None;
        let mut stage = None;
        for chunk in Chunks::of(body) {
            let chunk = chunk?;
            let mut fields = Fields::of(chunk.body);
            match chunk.id {
                VERTEX_MATERIAL_IDS if materials.is_none() => {
                    materials = Some(Ids::of(fields.u32s()?)?);
                }
                SHADER_IDS if shaders.is_none() => shaders = Some(Ids::of(fields.u32s()?)?),
                COLORS if colors.is_none() => colors = Some(fields.records::<4>()?),
                // The first stage only, as the slice draws one texture.
                TEXTURE_STAGE if stage.is_none() => {
                    let mut textures = None;
                    let mut coordinates = None;
                    for inner in Chunks::of(chunk.body) {
                        let inner = inner?;
                        let mut fields = Fields::of(inner.body);
                        match inner.id {
                            TEXTURE_IDS if textures.is_none() => {
                                textures = Some(Ids::of(fields.u32s()?)?);
                            }
                            STAGE_COORDINATES if coordinates.is_none() => {
                                coordinates = Some(
                                    fields
                                        .records::<8>()?
                                        .iter()
                                        .map(|record| {
                                            let at = |at: usize| {
                                                f32::from_le_bytes(
                                                    record[at..at + 4].try_into().expect("4 bytes"),
                                                )
                                            };
                                            [at(0), at(4)]
                                        })
                                        .collect(),
                                );
                            }
                            _ => {}
                        }
                    }
                    stage = Some(Stage {
                        textures: textures.unwrap_or(Ids::One(0)),
                        coordinates: coordinates.unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }
        Ok(Pass {
            materials: materials.unwrap_or(Ids::One(0)),
            shaders: shaders.unwrap_or(Ids::One(0)),
            colors,
            stage,
        })
    }
}

impl Ids {
    /// One id for all if the chunk holds one, else one each.
    fn of(ids: Vec<u32>) -> Result<Ids, W3dError> {
        match ids[..] {
            [] => Err(W3dError::Missing),
            [one] => Ok(Ids::One(one)),
            _ => Ok(Ids::Each(ids)),
        }
    }

    /// The id of vertex or triangle `at`.
    pub(crate) fn get(&self, at: usize) -> u32 {
        match self {
            Ids::One(id) => *id,
            Ids::Each(ids) => ids[at],
        }
    }
}

impl Stage {
    /// A triangle's texture id when it has none, as `read_texture_ids` skips it.
    pub(crate) const NONE: u32 = u32::MAX;

    /// The texture of triangle `at`, if it has one.
    pub(crate) fn texture(&self, at: usize) -> Option<u32> {
        Some(self.textures.get(at)).filter(|&id| id != Stage::NONE)
    }
}

impl Shader {
    /// Its blend, as a material file names it; `None` for a pair of factors no shipped shader
    /// uses.
    pub(crate) fn blend(self) -> Option<Blend> {
        Some(match (self.src_blend, self.dest_blend, self.alpha_test) {
            (ONE, ZERO, false) => Blend::Opaque,
            (ONE, ZERO, true) => Blend::Mask { cutoff: CUTOFF },
            (SRC_ALPHA, ONE_MINUS_SRC_ALPHA, false) => Blend::Alpha,
            (SRC_ALPHA, ONE_MINUS_SRC_ALPHA, true) => Blend::AlphaMask { cutoff: CUTOFF },
            (ONE, ONE, false) => Blend::Add,
            (ZERO, SRC_COLOR, false) => Blend::Multiply,
            (ONE, ONE_MINUS_SRC_COLOR, false) => Blend::Screen,
            _ => return None,
        })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;
    use crate::zero_hour::w3d_file::internals::{chunk, name};

    impl Shader {
        /// An opaque, textured shader that writes depth.
        pub(crate) const DEFAULT: Shader = Shader {
            depth_write: true,
            dest_blend: ZERO,
            src_blend: ONE,
            alpha_test: false,
            texturing: true,
        };
    }

    impl VertexMaterial {
        /// White in ambient and diffuse light, of no emissive light, opaque.
        pub(crate) const DEFAULT: VertexMaterial = VertexMaterial {
            ambient: [255; 3],
            diffuse: [255; 3],
            emissive: [0; 3],
            opacity: 1.0,
        };
    }

    /// A mesh header's flag of a skin, its vertices placed each by its bone.
    pub(crate) const SKIN_GEOMETRY: u32 = SKIN;

    impl W3dMesh {
        /// A mesh of `name`, `container.mesh` or `mesh`, of no vertices, drawn by the default
        /// shader and vertex material.
        pub(crate) fn named(name: &str) -> W3dMesh {
            W3dMesh {
                name: name.to_owned(),
                flags: 0,
                positions: Vec::new(),
                normals: Vec::new(),
                bones: None,
                triangles: Vec::new(),
                shaders: vec![Shader::DEFAULT],
                materials: vec![VertexMaterial::DEFAULT],
                textures: Vec::new(),
                pass: Pass {
                    materials: Ids::One(0),
                    shaders: Ids::One(0),
                    colors: None,
                    stage: None,
                },
            }
        }

        /// The mesh chunk the game reads as this mesh; names in the case given.
        pub(crate) fn chunk(&self) -> Vec<u8> {
            let (container, mesh) = self.name.split_once('.').unwrap_or(("", &self.name));
            let count = |len: usize| u32::try_from(len).unwrap().to_le_bytes();
            let mut header = [
                &3_u32.to_le_bytes()[..],
                &self.flags.to_le_bytes(),
                &name(mesh, NAME),
                &name(container, NAME),
                &count(self.triangles.len()),
                &count(self.positions.len()),
            ]
            .concat();
            // `W3dMeshHeader3Struct` is 116 bytes; the rest the importer does not read.
            header.resize(116, 0);
            let floats = |values: &[f32]| {
                values
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<u8>>()
            };
            let mut body = [
                chunk(HEADER, false, &header),
                chunk(VERTICES, false, &floats(self.positions.as_flattened())),
                chunk(NORMALS, false, &floats(self.normals.as_flattened())),
            ]
            .concat();
            if let Some(bones) = &self.bones {
                let records: Vec<u8> = bones
                    .iter()
                    .flat_map(|bone| [&bone.to_le_bytes()[..], &[0; 6]].concat())
                    .collect();
                body.extend(chunk(INFLUENCES, false, &records));
            }
            let triangles: Vec<u8> = self
                .triangles
                .iter()
                .flat_map(|triangle| {
                    let indices: Vec<u8> = triangle
                        .iter()
                        .flat_map(|index| index.to_le_bytes())
                        .collect();
                    // Its surface type, its normal and its distance, which the importer does not read.
                    [indices, vec![0; 20]].concat()
                })
                .collect();
            body.extend(chunk(TRIANGLES, false, &triangles));
            body.extend(self.material_chunks());
            body.extend(chunk(MATERIAL_PASS, true, &self.pass.chunks()));
            chunk(0, true, &body)
        }

        /// The chunks of its shaders, its vertex materials and its textures.
        fn material_chunks(&self) -> Vec<u8> {
            let shaders: Vec<u8> = self
                .shaders
                .iter()
                .flat_map(|shader| {
                    let mut record = [0; 16];
                    record[1] = u8::from(shader.depth_write);
                    record[3] = shader.dest_blend;
                    record[7] = shader.src_blend;
                    record[8] = u8::from(shader.texturing);
                    record[12] = u8::from(shader.alpha_test);
                    record
                })
                .collect();
            let materials: Vec<u8> = self
                .materials
                .iter()
                .flat_map(|material| {
                    let color = |rgb: [u8; 3]| [rgb[0], rgb[1], rgb[2], 0];
                    let info = [
                        &[0; 4][..],
                        &color(material.ambient),
                        &color(material.diffuse),
                        &[0; 4],
                        &color(material.emissive),
                        &0.0_f32.to_le_bytes(),
                        &material.opacity.to_le_bytes(),
                        &0.0_f32.to_le_bytes(),
                    ]
                    .concat();
                    chunk(
                        VERTEX_MATERIAL,
                        true,
                        &chunk(VERTEX_MATERIAL_INFO, false, &info),
                    )
                })
                .collect();
            let textures: Vec<u8> = self
                .textures
                .iter()
                .flat_map(|texture| {
                    let info = [&texture.flags.to_le_bytes()[..], &[0; 10]].concat();
                    let file = [texture.name.as_bytes(), &[0]].concat();
                    chunk(
                        TEXTURE,
                        true,
                        &[
                            chunk(TEXTURE_NAME, false, &file),
                            chunk(TEXTURE_INFO, false, &info),
                        ]
                        .concat(),
                    )
                })
                .collect();
            [
                chunk(SHADERS, false, &shaders),
                chunk(VERTEX_MATERIALS, true, &materials),
                chunk(TEXTURES, true, &textures),
            ]
            .concat()
        }
    }

    impl Pass {
        /// The chunks of a material pass's body.
        pub(crate) fn chunks(&self) -> Vec<u8> {
            let mut body = [
                chunk(VERTEX_MATERIAL_IDS, false, &self.materials.bytes()),
                chunk(SHADER_IDS, false, &self.shaders.bytes()),
            ]
            .concat();
            if let Some(colors) = &self.colors {
                body.extend(chunk(COLORS, false, colors.as_flattened()));
            }
            if let Some(stage) = &self.stage {
                let mut inner = chunk(TEXTURE_IDS, false, &stage.textures.bytes());
                if !stage.coordinates.is_empty() {
                    let coordinates: Vec<u8> = stage
                        .coordinates
                        .as_flattened()
                        .iter()
                        .flat_map(|value| value.to_le_bytes())
                        .collect();
                    inner.extend(chunk(STAGE_COORDINATES, false, &coordinates));
                }
                body.extend(chunk(TEXTURE_STAGE, true, &inner));
            }
            body
        }
    }

    impl Ids {
        fn bytes(&self) -> Vec<u8> {
            match self {
                Ids::One(id) => id.to_le_bytes().to_vec(),
                Ids::Each(ids) => ids.iter().flat_map(|id| id.to_le_bytes()).collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::w3d_file::internals::chunk;

    /// A textured skin of three vertices and two triangles, of every list the reader reads.
    fn skin() -> W3dMesh {
        W3dMesh {
            flags: SKIN | TWO_SIDED,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            bones: Some(vec![0, 1, 1]),
            triangles: vec![[0, 1, 2], [2, 1, 0]],
            shaders: vec![Shader::DEFAULT],
            materials: vec![VertexMaterial {
                ambient: [40, 50, 60],
                diffuse: [10, 20, 30],
                emissive: [1, 2, 3],
                opacity: 0.5,
            }],
            textures: vec![Texture {
                name: "rock.tga".to_owned(),
                flags: 0x8,
            }],
            pass: Pass {
                materials: Ids::One(0),
                shaders: Ids::Each(vec![0, 0]),
                colors: Some(vec![[1, 2, 3, 4]; 3]),
                stage: Some(Stage {
                    textures: Ids::Each(vec![0, Stage::NONE]),
                    coordinates: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                }),
            },
            ..W3dMesh::named("tank.skin")
        }
    }

    fn stage_of(mesh: &mut W3dMesh) -> &mut Stage {
        mesh.pass.stage.as_mut().unwrap()
    }

    /// The skin, changed by `change`, refused for `error`.
    fn refused(change: impl FnOnce(&mut W3dMesh), error: W3dError) {
        let mut mesh = skin();
        change(&mut mesh);
        assert_eq!(W3dMesh::read(&mesh.chunk()[8..]), Err(error), "{mesh:?}");
    }

    #[test]
    fn a_mesh_reads_as_written_and_its_lists_are_checked() {
        let mesh = skin();
        assert_eq!(W3dMesh::read(&mesh.chunk()[8..]).unwrap(), mesh);
        assert!(mesh.two_sided() && !mesh.hidden());
        let stage = mesh.pass.stage.as_ref().unwrap();
        assert_eq!([stage.texture(0), stage.texture(1)], [Some(0), None]);
        // An empty container's mesh is named by its own name alone; a stage may hold no
        // coordinates.
        let mut alone = W3dMesh {
            name: "rock01".to_owned(),
            ..skin()
        };
        stage_of(&mut alone).coordinates.clear();
        assert_eq!(W3dMesh::read(&alone.chunk()[8..]).unwrap(), alone);

        refused(|mesh| mesh.normals.truncate(2), W3dError::Counts);
        refused(|mesh| mesh.positions[1][2] = f32::NAN, W3dError::NotFinite);
        refused(
            |mesh| mesh.materials[0].opacity = f32::NAN,
            W3dError::NotFinite,
        );
        refused(|mesh| mesh.bones = None, W3dError::Counts);
        refused(
            |mesh| mesh.flags = 0x0001_0000,
            W3dError::Geometry(0x0001_0000),
        );
        refused(|mesh| mesh.triangles[1] = [0, 1, 3], W3dError::Index);
        refused(|mesh| mesh.pass.materials = Ids::One(1), W3dError::Index);
        // One id is one for all, as `read_shader_ids` reads it; else one for each triangle.
        refused(
            |mesh| mesh.pass.shaders = Ids::Each(vec![0; 3]),
            W3dError::Index,
        );
        refused(
            |mesh| mesh.pass.colors = Some(vec![[0; 4]; 2]),
            W3dError::Index,
        );
        // A triangle's texture `NONE` is untextured; one past the list, and `NONE` for all, are
        // refused, as `Peek_Texture` would read past its array.
        refused(
            |mesh| stage_of(mesh).textures = Ids::Each(vec![0, 1]),
            W3dError::Index,
        );
        refused(
            |mesh| stage_of(mesh).textures = Ids::One(Stage::NONE),
            W3dError::Index,
        );
        refused(
            |mesh| stage_of(mesh).coordinates.truncate(2),
            W3dError::Index,
        );
        assert_eq!(
            W3dMesh::read(&chunk(VERTICES, false, &[])),
            Err(W3dError::Missing)
        );
        // A mesh of no material pass: the skin's chunk without its last, the pass.
        let mesh = skin();
        let bytes = mesh.chunk();
        let pass = chunk(MATERIAL_PASS, true, &mesh.pass.chunks());
        assert_eq!(
            W3dMesh::read(&bytes[8..bytes.len() - pass.len()]),
            Err(W3dError::Missing)
        );
    }

    #[test]
    fn a_pass_keeps_the_first_chunk_of_each_list() {
        // Two of each list: the second is the game's alternate material, and the first is kept.
        let first = Pass {
            materials: Ids::One(1),
            shaders: Ids::Each(vec![2, 3]),
            colors: Some(vec![[9; 4]]),
            stage: Some(Stage {
                textures: Ids::One(4),
                coordinates: vec![[0.5, 0.25]],
            }),
        };
        let second = Pass {
            materials: Ids::One(5),
            shaders: Ids::One(6),
            colors: Some(vec![[7; 4]]),
            stage: Some(Stage {
                textures: Ids::One(8),
                coordinates: vec![[1.0, 1.0]],
            }),
        };
        // The first pass's chunks, then the second's: the second stage is not read at all.
        let mut body = first.chunks();
        body.extend(second.chunks());
        assert_eq!(W3dMesh::pass(&body).unwrap(), first);
        // Within one stage, the first texture ids and the first coordinates.
        let coordinates = |u: f32, v: f32| {
            chunk(
                STAGE_COORDINATES,
                false,
                &[u.to_le_bytes(), v.to_le_bytes()].concat(),
            )
        };
        let doubled = [
            chunk(TEXTURE_IDS, false, &4_u32.to_le_bytes()),
            chunk(TEXTURE_IDS, false, &8_u32.to_le_bytes()),
            coordinates(0.5, 0.25),
            coordinates(1.0, 1.0),
        ]
        .concat();
        assert_eq!(
            W3dMesh::pass(&chunk(TEXTURE_STAGE, true, &doubled))
                .unwrap()
                .stage,
            first.stage
        );
        // A pass of no lists has id 0 for all.
        let empty = W3dMesh::pass(&[]).unwrap();
        assert_eq!(
            (empty.materials, empty.shaders, empty.colors, empty.stage),
            (Ids::One(0), Ids::One(0), None, None)
        );
    }

    #[test]
    fn each_shipped_blend_is_a_mode_and_another_is_none() {
        let shader = |src_blend, dest_blend, alpha_test| Shader {
            dest_blend,
            src_blend,
            alpha_test,
            ..Shader::DEFAULT
        };
        let cutoff = 96.0 / 255.0;
        assert_eq!(shader(ONE, ZERO, false).blend(), Some(Blend::Opaque));
        assert_eq!(
            shader(ONE, ZERO, true).blend(),
            Some(Blend::Mask { cutoff })
        );
        assert_eq!(
            shader(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, false).blend(),
            Some(Blend::Alpha)
        );
        assert_eq!(
            shader(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, true).blend(),
            Some(Blend::AlphaMask { cutoff })
        );
        assert_eq!(shader(ONE, ONE, false).blend(), Some(Blend::Add));
        assert_eq!(
            shader(ZERO, SRC_COLOR, false).blend(),
            Some(Blend::Multiply)
        );
        assert_eq!(
            shader(ONE, ONE_MINUS_SRC_COLOR, false).blend(),
            Some(Blend::Screen)
        );
        assert_eq!(shader(ONE, ONE, true).blend(), None);
        assert_eq!(shader(SRC_ALPHA, ONE, false).blend(), None);
    }
}
