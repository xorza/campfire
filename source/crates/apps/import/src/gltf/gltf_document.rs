use serde::Serialize;

/// The subset of glTF 2.0's JSON a model of the importer uses: one scene, nodes, meshes of
/// triangles, materials, and textures by URI.
#[derive(Debug, Default, Serialize)]
pub(crate) struct GltfDocument {
    pub(crate) asset: Asset,
    pub(crate) scene: usize,
    pub(crate) scenes: Vec<Scene>,
    pub(crate) nodes: Vec<Node>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) meshes: Vec<Mesh>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) accessors: Vec<Accessor>,
    #[serde(rename = "bufferViews", skip_serializing_if = "Vec::is_empty")]
    pub(crate) buffer_views: Vec<BufferView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) buffers: Vec<Buffer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) materials: Vec<Material>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) textures: Vec<Texture>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) images: Vec<Image>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) samplers: Vec<Sampler>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Asset {
    pub(crate) version: &'static str,
    pub(crate) generator: &'static str,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Scene {
    pub(crate) nodes: Vec<usize>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Node {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) translation: Option<[f32; 3]>,
    /// A quaternion `x, y, z, w`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rotation: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mesh: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Mesh {
    pub(crate) name: String,
    pub(crate) primitives: Vec<Primitive>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Primitive {
    pub(crate) attributes: Attributes,
    pub(crate) indices: usize,
    pub(crate) material: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Attributes {
    #[serde(rename = "POSITION")]
    pub(crate) position: usize,
    #[serde(rename = "NORMAL")]
    pub(crate) normal: usize,
    #[serde(rename = "TEXCOORD_0", skip_serializing_if = "Option::is_none")]
    pub(crate) texcoord: Option<usize>,
    #[serde(rename = "COLOR_0", skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Accessor {
    #[serde(rename = "bufferView")]
    pub(crate) buffer_view: usize,
    #[serde(rename = "componentType")]
    pub(crate) component_type: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) normalized: bool,
    pub(crate) count: usize,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) min: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max: Option<[f32; 3]>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BufferView {
    pub(crate) buffer: usize,
    #[serde(rename = "byteOffset")]
    pub(crate) byte_offset: usize,
    #[serde(rename = "byteLength")]
    pub(crate) byte_length: usize,
    pub(crate) target: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct Buffer {
    #[serde(rename = "byteLength")]
    pub(crate) byte_length: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Material {
    pub(crate) name: String,
    #[serde(rename = "pbrMetallicRoughness")]
    pub(crate) pbr: Pbr,
    #[serde(rename = "emissiveFactor")]
    pub(crate) emissive: [f32; 3],
    #[serde(rename = "alphaMode")]
    pub(crate) alpha_mode: &'static str,
    #[serde(rename = "alphaCutoff", skip_serializing_if = "Option::is_none")]
    pub(crate) alpha_cutoff: Option<f32>,
    #[serde(rename = "doubleSided")]
    pub(crate) double_sided: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct Pbr {
    #[serde(rename = "baseColorFactor")]
    pub(crate) base_color: [f32; 4],
    #[serde(rename = "baseColorTexture", skip_serializing_if = "Option::is_none")]
    pub(crate) texture: Option<TextureRef>,
    #[serde(rename = "metallicFactor")]
    pub(crate) metallic: f32,
    #[serde(rename = "roughnessFactor")]
    pub(crate) roughness: f32,
}

#[derive(Debug, Serialize)]
pub(crate) struct TextureRef {
    pub(crate) index: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Texture {
    pub(crate) sampler: usize,
    pub(crate) source: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Image {
    pub(crate) uri: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Sampler {
    #[serde(rename = "wrapS")]
    pub(crate) wrap_s: u32,
    #[serde(rename = "wrapT")]
    pub(crate) wrap_t: u32,
}
