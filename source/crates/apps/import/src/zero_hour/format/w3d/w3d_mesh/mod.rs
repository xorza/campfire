use crate::zero_hour::format::w3d::error::W3dError;
use crate::zero_hour::format::w3d::w3d_chunks::W3dChunks;
use crate::zero_hour::format::w3d::w3d_fields::W3dFields;

/// A W3D mesh, what a model draws of it: its full name, its flags, its vertices, its triangles,
/// and its material passes, one at least, as `MeshModelClass` reads them. Every float but a texture
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
    pub(crate) passes: Vec<MaterialPass>,
}

/// How a mesh's pixels are colored, and how they meet what is drawn behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shader {
    pub(crate) depth_write: bool,
    pub(crate) dest_blend: u8,
    /// How the light of the vertices meets the texture, `PriGradientType`.
    pub(crate) primary_gradient: u8,
    /// How the detail stage's color meets the first stage's, `DetailColorFuncType`.
    pub(crate) detail_color: u8,
    pub(crate) src_blend: u8,
    pub(crate) alpha_test: bool,
    pub(crate) texturing: bool,
}

/// What a shader's first stage draws of a stage of no texture: the vertices' light; nothing, of
/// color and alpha 0; or the vertices' light of alpha 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NullTexture {
    Light,
    Clear,
    ClearLight,
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

/// A material pass: its vertex material, shader and color of each vertex or triangle, its first
/// texture stage, and its second, the detail stage, which its shaders' detail functions mix in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterialPass {
    /// By vertex.
    pub(crate) materials: Indexing,
    /// By triangle.
    pub(crate) shaders: Indexing,
    /// Each vertex's red, green, blue and alpha.
    pub(crate) colors: Option<Vec<[u8; 4]>>,
    pub(crate) stage: Option<Stage>,
    pub(crate) detail: Option<Stage>,
}

/// A texture stage: its texture of each triangle, [`Stage::NONE`] for a triangle with none, and
/// each vertex's `u, v`, `v` as the file holds it; none if the file holds none, where the game
/// samples every vertex at `0, 0`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stage {
    pub(crate) textures: Indexing,
    pub(crate) coordinates: Vec<[f32; 2]>,
}

/// An index into a list: one for all, or one for each vertex or triangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Indexing {
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
pub(crate) const CUTOFF: f32 = 96.0 / 255.0;

/// `GRADIENT_DISABLE`: the texture alone, its color and its alpha, as OpenGL's decal.
const GRADIENT_DISABLE: u8 = 0;

/// `GRADIENT_ADD`: the texture's color plus the vertices' light, its alpha times their opacity.
const GRADIENT_ADD: u8 = 2;

/// The detail color functions the install's shaders use: none, the first stage alone; scale, it
/// times the detail; add, it plus the detail.
pub(crate) const DETAIL_DISABLE: u8 = 0;
pub(crate) const DETAIL_SCALE: u8 = 2;
pub(crate) const DETAIL_ADD: u8 = 4;

/// The texture stages a pass holds, `MeshMatDescClass::MAX_TEX_STAGES`.
const STAGES: usize = 2;

/// W3D's blend factors, `W3DSHADER_SRCBLENDFUNC_*` and `W3DSHADER_DESTBLENDFUNC_*`.
pub(crate) const ZERO: u8 = 0;
pub(crate) const ONE: u8 = 1;
pub(crate) const SRC_ALPHA: u8 = 2;
pub(crate) const SRC_COLOR: u8 = 2;
pub(crate) const ONE_MINUS_SRC_COLOR: u8 = 3;
pub(crate) const ONE_MINUS_SRC_ALPHA: u8 = 5;

impl W3dMesh {
    pub(crate) fn read(body: &[u8]) -> Result<Self, W3dError> {
        let mut header = None;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut bones = None;
        let mut triangles = Vec::new();
        let mut shaders = Vec::new();
        let mut materials = Vec::new();
        let mut textures = Vec::new();
        let mut passes = Vec::new();
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            let mut fields = W3dFields::of(chunk.body);
            match chunk.id {
                HEADER => {
                    fields.cursor.u32()?;
                    let flags = fields.cursor.u32()?;
                    let name = fields.name(NAME)?;
                    let container = fields.name(NAME)?;
                    let triangles = fields.cursor.u32()?;
                    let vertices = fields.cursor.u32()?;
                    header = Some((flags, name, container, triangles, vertices));
                }
                VERTICES => positions = Self::vectors(&mut fields)?,
                NORMALS => normals = Self::vectors(&mut fields)?,
                INFLUENCES => {
                    let records = fields.records::<8>()?;
                    bones = Some(
                        records
                            .iter()
                            .map(|record| u16::from_le_bytes([record[0], record[1]]))
                            .collect(),
                    );
                }
                TRIANGLES => triangles = Self::triangles(&mut fields)?,
                SHADERS => shaders = Self::shaders(&mut fields)?,
                VERTEX_MATERIALS => {
                    for material in W3dChunks::of(chunk.body) {
                        let material = material?;
                        if material.id != VERTEX_MATERIAL {
                            continue;
                        }
                        let info = W3dChunks::of(material.body).first_of(VERTEX_MATERIAL_INFO)?;
                        materials.push(Self::vertex_material(info.body)?);
                    }
                }
                TEXTURES => {
                    for texture in W3dChunks::of(chunk.body) {
                        let texture = texture?;
                        if texture.id == TEXTURE {
                            textures.push(Self::texture(texture.body)?);
                        }
                    }
                }
                MATERIAL_PASS => passes.push(Self::pass(chunk.body)?),
                _ => {}
            }
        }
        let (flags, name, container, triangle_count, vertex_count) =
            header.ok_or(W3dError::Missing)?;
        let mesh = Self {
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
            passes,
        };
        // A mesh of no pass is drawn by no material; none ships.
        if mesh.passes.is_empty() {
            return Err(W3dError::Missing);
        }
        mesh.check(triangle_count, vertex_count)?;
        Ok(mesh)
    }

    /// Whether the game draws it hidden until a state shows it.
    pub(crate) const fn is_hidden(&self) -> bool {
        self.flags & HIDDEN != 0
    }

    pub(crate) const fn is_two_sided(&self) -> bool {
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
        let ids = |ids: &Indexing, each: usize, len: usize| match ids {
            Indexing::One(id) => below(*id, len),
            Indexing::Each(ids) => ids.len() == each && ids.iter().all(|&id| below(id, len)),
        };
        let stage = |stage: &Option<Stage>| {
            stage.as_ref().is_none_or(|stage| {
                let textures = match &stage.textures {
                    Indexing::One(id) => below(*id, self.textures.len()),
                    Indexing::Each(ids) => {
                        ids.len() == triangles
                            && ids
                                .iter()
                                .all(|&id| id == Stage::NONE || below(id, self.textures.len()))
                    }
                };
                textures && (stage.coordinates.is_empty() || per_vertex(stage.coordinates.len()))
            })
        };
        let pass_holds = |pass: &MaterialPass| {
            let colors = pass
                .colors
                .as_ref()
                .is_none_or(|colors| per_vertex(colors.len()));
            ids(&pass.materials, vertices, self.materials.len())
                && ids(&pass.shaders, triangles, self.shaders.len())
                && colors
                && stage(&pass.stage)
                && stage(&pass.detail)
        };
        if !self.passes.iter().all(pass_holds) {
            return Err(W3dError::Index);
        }
        Ok(())
    }

    /// `W3dTriStruct`s: each triangle's three vertices, then what the importer does not read.
    fn triangles(fields: &mut W3dFields<'_>) -> Result<Vec<[u32; 3]>, W3dError> {
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
    fn shaders(fields: &mut W3dFields<'_>) -> Result<Vec<Shader>, W3dError> {
        Ok(fields
            .records::<16>()?
            .iter()
            .map(|shader| Shader {
                depth_write: shader[1] != 0,
                dest_blend: shader[3],
                primary_gradient: shader[5],
                src_blend: shader[7],
                detail_color: shader[9],
                alpha_test: shader[12] != 0,
                texturing: shader[8] != 0,
            })
            .collect())
    }

    fn vectors(fields: &mut W3dFields<'_>) -> Result<Vec<[f32; 3]>, W3dError> {
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
        let mut fields = W3dFields::of(body);
        fields.cursor.u32()?;
        let color = |fields: &mut W3dFields<'_>| -> Result<[u8; 3], W3dError> {
            let bytes = fields.cursor.take(4)?;
            Ok([bytes[0], bytes[1], bytes[2]])
        };
        let ambient = color(&mut fields)?;
        let diffuse = color(&mut fields)?;
        color(&mut fields)?;
        let emissive = color(&mut fields)?;
        fields.cursor.f32()?;
        let opacity = fields.cursor.f32()?;
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
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            let mut fields = W3dFields::of(chunk.body);
            match chunk.id {
                TEXTURE_NAME => name = Some(fields.name(chunk.body.len())?),
                TEXTURE_INFO => flags = fields.cursor.u16()?,
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
    fn pass(body: &[u8]) -> Result<MaterialPass, W3dError> {
        let mut materials = None;
        let mut shaders = None;
        let mut colors = None;
        let mut stages = Vec::with_capacity(STAGES);
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            let mut fields = W3dFields::of(chunk.body);
            match chunk.id {
                VERTEX_MATERIAL_IDS if materials.is_none() => {
                    materials = Some(Indexing::of(fields.u32s()?)?);
                }
                SHADER_IDS if shaders.is_none() => shaders = Some(Indexing::of(fields.u32s()?)?),
                COLORS if colors.is_none() => colors = Some(fields.records::<4>()?),
                // Each stage is the next, as `read_texture_stage` counts them; the game holds two.
                TEXTURE_STAGE if stages.len() == STAGES => return Err(W3dError::Stages),
                TEXTURE_STAGE => {
                    let mut textures = None;
                    let mut coordinates = None;
                    for inner in W3dChunks::of(chunk.body) {
                        let inner = inner?;
                        let mut fields = W3dFields::of(inner.body);
                        match inner.id {
                            TEXTURE_IDS if textures.is_none() => {
                                textures = Some(Indexing::of(fields.u32s()?)?);
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
                    stages.push(Stage {
                        textures: textures.unwrap_or(Indexing::One(0)),
                        coordinates: coordinates.unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }
        let mut stages = stages.into_iter();
        Ok(MaterialPass {
            materials: materials.unwrap_or(Indexing::One(0)),
            shaders: shaders.unwrap_or(Indexing::One(0)),
            colors,
            stage: stages.next(),
            detail: stages.next(),
        })
    }
}

impl Indexing {
    /// One id for all if the chunk holds one, else one each.
    fn of(ids: impl IntoIterator<Item = u32>) -> Result<Self, W3dError> {
        let ids: Vec<u32> = ids.into_iter().collect();
        match ids[..] {
            [] => Err(W3dError::Missing),
            [one] => Ok(Self::One(one)),
            _ => Ok(Self::Each(ids)),
        }
    }

    /// The id of vertex or triangle `at`.
    pub(crate) fn get(&self, at: usize) -> u32 {
        match self {
            Self::One(id) => *id,
            Self::Each(ids) => ids[at],
        }
    }
}

impl Stage {
    /// A triangle's texture id when it has none, as `read_texture_ids` skips it.
    pub(crate) const NONE: u32 = u32::MAX;

    /// The texture of triangle `at`, if it has one.
    pub(crate) fn texture(&self, at: usize) -> Option<u32> {
        Some(self.textures.get(at)).filter(|&id| id != Self::NONE)
    }
}

impl Shader {
    /// Whether it draws its texture alone, of no light, color or opacity of the vertices
    /// (`ShaderClass::Apply`); with no texture, the game draws the vertices' light.
    pub(crate) const fn is_unlit(self) -> bool {
        self.texturing && self.primary_gradient == GRADIENT_DISABLE
    }

    /// What its first stage draws of a stage that holds no texture
    /// (`TextureBaseClass::Apply_Null`), which native Direct3D samples as the null vector, `(0, 0,
    /// 0, 0)`: with texturing off, the vertices' light, as of any triangle; with it on, an alpha of
    /// 0, and a color of 0, but the vertices' light under `GRADIENT_ADD`, which adds it to the
    /// texture's.
    pub(crate) const fn null_texture(self) -> NullTexture {
        match (self.texturing, self.primary_gradient) {
            (false, _) => NullTexture::Light,
            (true, GRADIENT_ADD) => NullTexture::ClearLight,
            (true, _) => NullTexture::Clear,
        }
    }
}

#[cfg(test)]
pub(crate) mod samples;

#[cfg(test)]
mod tests;
