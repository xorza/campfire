use campfire_codec::Json;
use campfire_common::ErrorReport;
use gltf_json::accessor::{ComponentType, GenericComponentType, Type};
use gltf_json::buffer::{Target, View};
use gltf_json::texture::{Sampler, WrappingMode};
use gltf_json::validation::{Checked, USize64, Validate};
use gltf_json::{
    Accessor, Asset, Buffer, Extras, Image, Index, Material, Path, Root, Scene, Texture,
};

/// A glTF document of `gltf-json`'s schema and its one buffer, which `encode` writes as a GLB
/// file: a 12-byte header, the JSON chunk padded with spaces, and the buffer's chunk padded with
/// zeros, each to 4 bytes.
#[derive(Debug)]
pub(crate) struct GlbBuilder {
    pub(crate) document: Root,
    buffer: Vec<u8>,
}

/// What an accessor's elements are, and the target of their view.
#[derive(Debug, Clone, Copy)]
struct Elements {
    component: ComponentType,
    kind: Type,
    /// Whether an integer component reads as 0 to 1.
    normalized: bool,
    target: Target,
}

impl Elements {
    const VEC3: Self = Self::floats(Type::Vec3);
    const VEC2: Self = Self::floats(Type::Vec2);
    const COLORS: Self = Self {
        component: ComponentType::U8,
        kind: Type::Vec4,
        normalized: true,
        target: Target::ArrayBuffer,
    };
    const INDICES: Self = Self {
        component: ComponentType::U32,
        kind: Type::Scalar,
        normalized: false,
        target: Target::ElementArrayBuffer,
    };

    const fn floats(kind: Type) -> Self {
        Self {
            component: ComponentType::F32,
            kind,
            normalized: false,
            target: Target::ArrayBuffer,
        }
    }
}

/// GLB's magic, `glTF`, its version, and its chunks' types, `JSON` and `BIN\0`.
const MAGIC: u32 = 0x4654_6C67;
const VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

impl GlbBuilder {
    pub(crate) fn new() -> Self {
        let mut document = Root {
            asset: Asset {
                version: String::from("2.0"),
                generator: Some(String::from("campfire-import")),
                ..Asset::default()
            },
            ..Root::default()
        };
        let scene = document.push(Scene {
            extensions: None,
            extras: Extras::default(),
            name: None,
            nodes: Vec::new(),
        });
        document.scene = Some(scene);
        Self {
            document,
            buffer: Vec::new(),
        }
    }

    /// The document's one scene.
    pub(crate) fn scene(&mut self) -> &mut Scene {
        &mut self.document.scenes[0]
    }

    /// An accessor of `vectors`, with their bounds, as glTF requires of positions.
    pub(crate) fn positions(&mut self, vectors: &[[f32; 3]]) -> Index<Accessor> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vector in vectors {
            for axis in 0..3 {
                min[axis] = min[axis].min(vector[axis]);
                max[axis] = max[axis].max(vector[axis]);
            }
        }
        let accessor = self.floats(
            vectors.iter().flatten().copied(),
            vectors.len(),
            Elements::VEC3,
        );
        let bounds = &mut self.document.accessors[accessor.value()];
        bounds.min = Some(Json::convert(&min).expect("three floats are JSON"));
        bounds.max = Some(Json::convert(&max).expect("three floats are JSON"));
        accessor
    }

    /// An accessor of `vectors`.
    pub(crate) fn vectors(&mut self, vectors: &[[f32; 3]]) -> Index<Accessor> {
        self.floats(
            vectors.iter().flatten().copied(),
            vectors.len(),
            Elements::VEC3,
        )
    }

    pub(crate) fn coordinates(&mut self, coordinates: &[[f32; 2]]) -> Index<Accessor> {
        let floats = coordinates.iter().flatten().copied();
        self.floats(floats, coordinates.len(), Elements::VEC2)
    }

    /// An accessor of colors, each 4 bytes read as 0 to 1.
    pub(crate) fn colors(&mut self, colors: &[[u8; 4]]) -> Index<Accessor> {
        let bytes = colors.iter().flatten().copied().collect();
        self.accessor(bytes, colors.len(), Elements::COLORS)
    }

    pub(crate) fn indices(&mut self, indices: &[u32]) -> Index<Accessor> {
        let bytes = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        self.accessor(bytes, indices.len(), Elements::INDICES)
    }

    /// A texture of the image at `uri`, wrapped as `wrap_s` and `wrap_t`.
    pub(crate) fn texture(
        &mut self,
        uri: String,
        wrap_s: WrappingMode,
        wrap_t: WrappingMode,
    ) -> Index<Texture> {
        let document = &mut self.document;
        let source = document.push(Image {
            buffer_view: None,
            mime_type: None,
            name: None,
            uri: Some(uri),
            extensions: None,
            extras: Extras::default(),
        });
        let sampler = document.push(Sampler {
            wrap_s: Checked::Valid(wrap_s),
            wrap_t: Checked::Valid(wrap_t),
            ..Sampler::default()
        });
        document.push(Texture {
            name: None,
            sampler: Some(sampler),
            source,
            extensions: None,
            extras: Extras::default(),
        })
    }

    /// `material`'s index, with each extension it uses in `extensionsUsed`, in name order.
    pub(crate) fn material(&mut self, material: Material) -> Index<Material> {
        if let Some(extensions) = &material.extensions {
            let specular = extensions
                .specular
                .is_some()
                .then_some("KHR_materials_specular");
            let unlit = extensions.unlit.is_some().then_some("KHR_materials_unlit");
            let others = extensions.others.keys().map(String::as_str);
            let used = &mut self.document.extensions_used;
            for extension in specular.into_iter().chain(unlit).chain(others) {
                if let Err(at) = used.binary_search_by(|known| known.as_str().cmp(extension)) {
                    used.insert(at, extension.to_owned());
                }
            }
        }
        self.document.push(material)
    }

    /// The GLB file's bytes. A debug build checks the document against glTF's schema first.
    pub(crate) fn encode(mut self) -> Vec<u8> {
        if let Some(buffer) = self.document.buffers.first_mut() {
            buffer.byte_length = USize64::from(self.buffer.len());
        }
        debug_assert_eq!(
            Self::problems(&self.document),
            Vec::<String>::new(),
            "the importer writes a valid glTF"
        );
        let mut json = Json::write(&self.document).into_bytes();
        json.resize(json.len().next_multiple_of(4), b' ');
        let mut buffer = self.buffer;
        buffer.resize(buffer.len().next_multiple_of(4), 0);
        let word = |value: usize| {
            u32::try_from(value)
                .expect("a model under 4 GiB")
                .to_le_bytes()
        };
        let chunks = 8
            + json.len()
            + if buffer.is_empty() {
                0
            } else {
                8 + buffer.len()
            };
        let mut out = [
            MAGIC.to_le_bytes(),
            VERSION.to_le_bytes(),
            word(12 + chunks),
        ]
        .concat();
        out.extend(word(json.len()));
        out.extend(JSON_CHUNK.to_le_bytes());
        out.extend(json);
        if !buffer.is_empty() {
            out.extend(word(buffer.len()));
            out.extend(BIN_CHUNK.to_le_bytes());
            out.extend(buffer);
        }
        out
    }

    /// Each problem glTF's schema finds in `document`, by its path.
    fn problems(document: &Root) -> Vec<String> {
        let mut problems = Vec::new();
        document.validate(document, Path::new, &mut |path, error| {
            problems.push(format!("{}: {}", path(), ErrorReport::of(&error)));
        });
        problems
    }

    fn floats(
        &mut self,
        floats: impl Iterator<Item = f32>,
        count: usize,
        elements: Elements,
    ) -> Index<Accessor> {
        let bytes = floats.flat_map(f32::to_le_bytes).collect();
        self.accessor(bytes, count, elements)
    }

    /// An accessor of `count` `elements`, whose `bytes` its own view holds, at the buffer's end
    /// aligned to 4, as every accessor's components need. The first adds the one buffer, whose
    /// length `encode` sets.
    fn accessor(&mut self, bytes: Vec<u8>, count: usize, elements: Elements) -> Index<Accessor> {
        if self.document.buffers.is_empty() {
            self.document.push(Buffer {
                byte_length: USize64(0),
                name: None,
                uri: None,
                extensions: None,
                extras: Extras::default(),
            });
        }
        self.buffer.resize(self.buffer.len().next_multiple_of(4), 0);
        let view = self.document.push(View {
            buffer: Index::new(0),
            byte_length: USize64::from(bytes.len()),
            byte_offset: Some(USize64::from(self.buffer.len())),
            byte_stride: None,
            name: None,
            target: Some(Checked::Valid(elements.target)),
            extensions: None,
            extras: Extras::default(),
        });
        self.buffer.extend(bytes);
        self.document.push(Accessor {
            buffer_view: Some(view),
            byte_offset: None,
            count: USize64::from(count),
            component_type: Checked::Valid(GenericComponentType(elements.component)),
            extensions: None,
            extras: Extras::default(),
            type_: Checked::Valid(elements.kind),
            min: None,
            max: None,
            name: None,
            normalized: elements.normalized,
            sparse: None,
        })
    }
}

#[cfg(test)]
pub(crate) mod reach_in {
    use super::*;

    impl GlbBuilder {
        /// The bytes of `accessor`, as its view holds them.
        fn bytes_of(&self, accessor: Index<Accessor>) -> &[u8] {
            let view = self.document.accessors[accessor.value()]
                .buffer_view
                .expect("an accessor of a view");
            let view = &self.document.buffer_views[view.value()];
            let start = usize::try_from(view.byte_offset.unwrap_or_default().0).unwrap();
            let len = usize::try_from(view.byte_length.0).unwrap();
            &self.buffer[start..start + len]
        }

        pub(crate) fn floats_of(&self, accessor: Index<Accessor>) -> Vec<f32> {
            let (words, rest) = self.bytes_of(accessor).as_chunks::<4>();
            assert!(rest.is_empty());
            words.iter().copied().map(f32::from_le_bytes).collect()
        }

        pub(crate) fn indices_of(&self, accessor: Index<Accessor>) -> Vec<u32> {
            let (words, rest) = self.bytes_of(accessor).as_chunks::<4>();
            assert!(rest.is_empty());
            words.iter().copied().map(u32::from_le_bytes).collect()
        }

        pub(crate) fn colors_of(&self, accessor: Index<Accessor>) -> Vec<u8> {
            self.bytes_of(accessor).to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glb_holds_its_json_and_its_buffer_each_padded_to_4_bytes() {
        let mut glb = GlbBuilder::new();
        let position = glb.positions(&[[1.0, 2.0, 3.0], [4.0, -5.0, 6.0]]);
        let color = glb.colors(&[[1, 2, 3, 4]]);
        let texture = glb.texture(
            "../a.ktx2".to_owned(),
            WrappingMode::ClampToEdge,
            WrappingMode::Repeat,
        );
        assert_eq!(
            [position.value(), color.value(), texture.value()],
            [0, 1, 0]
        );
        assert_eq!(glb.floats_of(position), [1.0, 2.0, 3.0, 4.0, -5.0, 6.0]);
        assert_eq!(glb.colors_of(color), [1, 2, 3, 4]);
        let json = concat!(
            r#"{"accessors":[{"bufferView":0,"count":2,"componentType":5126,"type":"VEC3","min":[1.0,-5.0,3.0],"max":[4.0,2.0,6.0]},"#,
            r#"{"bufferView":1,"count":1,"componentType":5121,"type":"VEC4","normalized":true}],"#,
            r#""asset":{"generator":"campfire-import","version":"2.0"},"buffers":[{"byteLength":28}],"#,
            r#""bufferViews":[{"buffer":0,"byteLength":24,"byteOffset":0,"target":34962},{"buffer":0,"byteLength":4,"byteOffset":24,"target":34962}],"#,
            r#""scene":0,"images":[{"uri":"../a.ktx2"}],"samplers":[{"wrapS":33071,"wrapT":10497}],"scenes":[{}],"#,
            r#""textures":[{"sampler":0,"source":0}]}"#,
        );
        // The JSON padded with spaces to 4 bytes; the buffer, 28 bytes, needs none.
        let padded = json.len().next_multiple_of(4);
        let word = |value: usize| u32::try_from(value).unwrap().to_le_bytes();
        let bytes = glb.encode();
        assert_eq!(bytes[..4], *b"glTF");
        assert_eq!(bytes[4..8], word(2));
        assert_eq!(bytes[8..12], word(bytes.len()));
        assert_eq!(bytes[12..16], word(padded));
        assert_eq!(bytes[16..20], *b"JSON");
        let text = &bytes[20..20 + padded];
        assert_eq!(str::from_utf8(&text[..json.len()]).unwrap(), json);
        assert!(text[json.len()..].iter().all(|&byte| byte == b' '));
        assert_eq!(bytes.len(), 12 + 8 + padded + 8 + 28);
        let bin = &bytes[20 + padded..];
        assert_eq!(bin[..4], word(28));
        assert_eq!(bin[4..8], *b"BIN\0");
        assert_eq!(bin[8..12], 1.0_f32.to_le_bytes());
        assert_eq!(bin[32..], [1, 2, 3, 4]);
        // A model of no buffer, as one of boxes alone, holds no buffer and no BIN chunk.
        let empty = GlbBuilder::new().encode();
        let json =
            r#"{"asset":{"generator":"campfire-import","version":"2.0"},"scene":0,"scenes":[{}]}"#;
        assert_eq!(empty.len(), 12 + 8 + json.len().next_multiple_of(4));
        assert_eq!(str::from_utf8(&empty[20..20 + json.len()]).unwrap(), json);
    }

    /// A document that breaks glTF's schema, a primitive's accessor past the last, is found.
    #[test]
    fn a_document_against_gltf_s_schema_is_found() {
        let mut glb = GlbBuilder::new();
        glb.positions(&[[0.0; 3]]);
        assert!(GlbBuilder::problems(&glb.document).is_empty());
        glb.document.accessors[0].buffer_view = Some(Index::new(5));
        assert_eq!(
            GlbBuilder::problems(&glb.document),
            ["accessors[0].bufferView: Index out of bounds"]
        );
    }
}
