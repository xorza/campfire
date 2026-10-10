use campfire_common::Json;

use crate::gltf::gltf_document::{
    Accessor, Asset, Buffer, BufferView, GltfDocument, Image, Sampler, Scene, Texture,
};

/// A glTF document and its one buffer, which `encode` writes as a GLB file: a 12-byte header,
/// the JSON chunk padded with spaces, and the buffer's chunk padded with zeros, each to 4 bytes.
#[derive(Debug)]
pub(crate) struct GlbBuilder {
    pub(crate) document: GltfDocument,
    buffer: Vec<u8>,
}

/// glTF's component types and buffer targets.
const FLOAT: u32 = 5126;
const UNSIGNED_BYTE: u32 = 5121;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// A sampler's wraps.
pub(crate) const REPEAT: u32 = 10497;
pub(crate) const CLAMP_TO_EDGE: u32 = 33071;

/// GLB's magic, `glTF`, its version, and its chunks' types, `JSON` and `BIN\0`.
const MAGIC: u32 = 0x4654_6C67;
const VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

impl GlbBuilder {
    pub(crate) fn new() -> GlbBuilder {
        GlbBuilder {
            document: GltfDocument {
                asset: Asset {
                    version: "2.0",
                    generator: "campfire-import",
                },
                scene: 0,
                scenes: vec![Scene::default()],
                ..GltfDocument::default()
            },
            buffer: Vec::new(),
        }
    }

    /// An accessor of `vectors`, with their bounds, as glTF requires of positions.
    pub(crate) fn positions(&mut self, vectors: &[[f32; 3]]) -> usize {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vector in vectors {
            for axis in 0..3 {
                min[axis] = min[axis].min(vector[axis]);
                max[axis] = max[axis].max(vector[axis]);
            }
        }
        let accessor = self.floats(vectors.iter().flatten().copied(), vectors.len(), "VEC3");
        let bounds = &mut self.document.accessors[accessor];
        bounds.min = Some(min);
        bounds.max = Some(max);
        accessor
    }

    /// An accessor of `vectors`.
    pub(crate) fn vectors(&mut self, vectors: &[[f32; 3]]) -> usize {
        self.floats(vectors.iter().flatten().copied(), vectors.len(), "VEC3")
    }

    pub(crate) fn coordinates(&mut self, coordinates: &[[f32; 2]]) -> usize {
        self.floats(
            coordinates.iter().flatten().copied(),
            coordinates.len(),
            "VEC2",
        )
    }

    /// An accessor of colors, each 4 bytes read as 0 to 1.
    pub(crate) fn colors(&mut self, colors: &[[u8; 4]]) -> usize {
        let view = self.view(colors.iter().flatten().copied().collect(), ARRAY_BUFFER);
        self.accessor(view, UNSIGNED_BYTE, true, colors.len(), "VEC4")
    }

    pub(crate) fn indices(&mut self, indices: &[u32]) -> usize {
        let bytes = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        let view = self.view(bytes, ELEMENT_ARRAY_BUFFER);
        self.accessor(view, UNSIGNED_INT, false, indices.len(), "SCALAR")
    }

    /// A texture of the image at `uri`, wrapped as `wrap_s` and `wrap_t`.
    pub(crate) fn texture(&mut self, uri: String, wrap_s: u32, wrap_t: u32) -> usize {
        let document = &mut self.document;
        document.images.push(Image { uri });
        document.samplers.push(Sampler { wrap_s, wrap_t });
        document.textures.push(Texture {
            sampler: document.samplers.len() - 1,
            source: document.images.len() - 1,
        });
        document.textures.len() - 1
    }

    /// The GLB file's bytes.
    pub(crate) fn encode(mut self) -> Vec<u8> {
        if !self.buffer.is_empty() {
            self.document.buffers.push(Buffer {
                byte_length: self.buffer.len(),
            });
        }
        let mut json = Json::write(&self.document)
            .expect("a glTF document writes as JSON")
            .into_bytes();
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

    fn floats(
        &mut self,
        floats: impl Iterator<Item = f32>,
        count: usize,
        kind: &'static str,
    ) -> usize {
        let view = self.view(floats.flat_map(f32::to_le_bytes).collect(), ARRAY_BUFFER);
        self.accessor(view, FLOAT, false, count, kind)
    }

    /// A buffer view of `bytes`, at the buffer's end aligned to 4, as every accessor's
    /// components need.
    fn view(&mut self, bytes: Vec<u8>, target: u32) -> usize {
        self.buffer.resize(self.buffer.len().next_multiple_of(4), 0);
        self.document.buffer_views.push(BufferView {
            buffer: 0,
            byte_offset: self.buffer.len(),
            byte_length: bytes.len(),
            target,
        });
        self.buffer.extend(bytes);
        self.document.buffer_views.len() - 1
    }

    fn accessor(
        &mut self,
        view: usize,
        component_type: u32,
        normalized: bool,
        count: usize,
        kind: &'static str,
    ) -> usize {
        self.document.accessors.push(Accessor {
            buffer_view: view,
            component_type,
            normalized,
            count,
            kind,
            min: None,
            max: None,
        });
        self.document.accessors.len() - 1
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;

    impl GlbBuilder {
        /// The bytes of `accessor`, as its view holds them.
        fn bytes_of(&self, accessor: usize) -> &[u8] {
            let view = &self.document.buffer_views[self.document.accessors[accessor].buffer_view];
            &self.buffer[view.byte_offset..view.byte_offset + view.byte_length]
        }

        pub(crate) fn floats_of(&self, accessor: usize) -> Vec<f32> {
            let (words, rest) = self.bytes_of(accessor).as_chunks::<4>();
            assert!(rest.is_empty());
            words.iter().copied().map(f32::from_le_bytes).collect()
        }

        pub(crate) fn indices_of(&self, accessor: usize) -> Vec<u32> {
            let (words, rest) = self.bytes_of(accessor).as_chunks::<4>();
            assert!(rest.is_empty());
            words.iter().copied().map(u32::from_le_bytes).collect()
        }

        pub(crate) fn colors_of(&self, accessor: usize) -> Vec<u8> {
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
        let texture = glb.texture("../a.ktx2".to_owned(), CLAMP_TO_EDGE, REPEAT);
        assert_eq!((position, color, texture), (0, 1, 0));
        assert_eq!(glb.floats_of(position), [1.0, 2.0, 3.0, 4.0, -5.0, 6.0]);
        assert_eq!(glb.colors_of(color), [1, 2, 3, 4]);
        let json = concat!(
            r#"{"asset":{"version":"2.0","generator":"campfire-import"},"scene":0,"scenes":[{"nodes":[]}],"nodes":[],"#,
            r#""accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"VEC3","min":[1.0,-5.0,3.0],"max":[4.0,2.0,6.0]},"#,
            r#"{"bufferView":1,"componentType":5121,"normalized":true,"count":1,"type":"VEC4"}],"#,
            r#""bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":24,"target":34962},{"buffer":0,"byteOffset":24,"byteLength":4,"target":34962}],"#,
            r#""buffers":[{"byteLength":28}],"textures":[{"sampler":0,"source":0}],"images":[{"uri":"../a.ktx2"}],"#,
            r#""samplers":[{"wrapS":33071,"wrapT":10497}]}"#,
        );
        // The JSON padded with spaces to 4 bytes; the buffer, 28 bytes, needs none.
        let padded = json.len().next_multiple_of(4);
        let word = |value: usize| u32::try_from(value).unwrap().to_le_bytes();
        let bytes = glb.encode();
        assert_eq!(bytes[..4], *b"glTF");
        assert_eq!(bytes[4..8], word(2));
        assert_eq!(bytes[8..12], word(bytes.len()));
        assert_eq!(bytes.len(), 12 + 8 + padded + 8 + 28);
        assert_eq!(bytes[12..16], word(padded));
        assert_eq!(bytes[16..20], *b"JSON");
        let text = &bytes[20..20 + padded];
        assert_eq!(&text[..json.len()], json.as_bytes());
        assert!(text[json.len()..].iter().all(|&byte| byte == b' '));
        let bin = &bytes[20 + padded..];
        assert_eq!(bin[..4], word(28));
        assert_eq!(bin[4..8], *b"BIN\0");
        assert_eq!(bin[8..12], 1.0_f32.to_le_bytes());
        assert_eq!(bin[32..], [1, 2, 3, 4]);
        // A model of no buffer, as one of boxes alone, holds no buffer and no BIN chunk.
        let empty = GlbBuilder::new().encode();
        let json = r#"{"asset":{"version":"2.0","generator":"campfire-import"},"scene":0,"scenes":[{"nodes":[]}],"nodes":[]}"#;
        assert_eq!(empty.len(), 12 + 8 + json.len().next_multiple_of(4));
        assert_eq!(&empty[20..20 + json.len()], json.as_bytes());
    }
}
