use super::*;
use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;
use crate::zero_hour::format::w3d::w3d_fields::samples::name;

impl Shader {
    /// An opaque, textured shader that writes depth.
    pub(crate) const DEFAULT: Self = Self {
        depth_write: true,
        dest_blend: ZERO,
        primary_gradient: GRADIENT_MODULATE,
        detail_color: DETAIL_DISABLE,
        src_blend: ONE,
        alpha_test: false,
        texturing: true,
    };
}

impl VertexMaterial {
    /// White in ambient and diffuse light, of no emissive light, opaque.
    pub(crate) const DEFAULT: Self = Self {
        ambient: [255; 3],
        diffuse: [255; 3],
        emissive: [0; 3],
        opacity: 1.0,
    };
}

/// `GRADIENT_MODULATE`: the texture times the vertices' light.
pub(crate) const GRADIENT_MODULATE: u8 = 1;

/// A mesh header's flag of a skin, its vertices placed each by its bone.
pub(crate) const SKIN_GEOMETRY: u32 = SKIN;

impl W3dMesh {
    /// A mesh of `name`, `container.mesh` or `mesh`, of no vertices, drawn by the default
    /// shader and vertex material.
    pub(crate) fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            flags: 0,
            positions: Vec::new(),
            normals: Vec::new(),
            bones: None,
            triangles: Vec::new(),
            shaders: vec![Shader::DEFAULT],
            materials: vec![VertexMaterial::DEFAULT],
            textures: Vec::new(),
            passes: vec![MaterialPass {
                materials: Indexing::One(0),
                shaders: Indexing::One(0),
                colors: None,
                stage: None,
                detail: None,
            }],
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
                // Its surface type, its normal and its distance, which the importer does not
                // read.
                [indices, vec![0; 20]].concat()
            })
            .collect();
        body.extend(chunk(TRIANGLES, false, &triangles));
        body.extend(self.material_chunks());
        for pass in &self.passes {
            body.extend(chunk(MATERIAL_PASS, true, &pass.chunks()));
        }
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
                record[5] = shader.primary_gradient;
                record[7] = shader.src_blend;
                record[9] = shader.detail_color;
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

impl MaterialPass {
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
        for stage in self.stage.iter().chain(&self.detail) {
            body.extend(stage.chunk());
        }
        body
    }
}

impl Stage {
    /// Its chunk, a texture stage's.
    pub(crate) fn chunk(&self) -> Vec<u8> {
        let mut inner = chunk(TEXTURE_IDS, false, &self.textures.bytes());
        if !self.coordinates.is_empty() {
            let coordinates: Vec<u8> = self
                .coordinates
                .as_flattened()
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            inner.extend(chunk(STAGE_COORDINATES, false, &coordinates));
        }
        chunk(TEXTURE_STAGE, true, &inner)
    }
}

impl Indexing {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::One(id) => id.to_le_bytes().to_vec(),
            Self::Each(ids) => ids.iter().flat_map(|id| id.to_le_bytes()).collect(),
        }
    }
}
