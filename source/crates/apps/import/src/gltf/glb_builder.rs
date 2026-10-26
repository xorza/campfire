use campfire_codec::Json;
use campfire_common::ErrorReport;
use gltf_json::accessor::{ComponentType, GenericComponentType, Type};
use gltf_json::buffer::{Target, View};
use gltf_json::texture::{Sampler, WrappingMode};
use gltf_json::validation::{Checked, USize64, Validate};
use gltf_json::{
    Accessor, Animation, Asset, Buffer, Extras, Image, Index, Material, Node, Path, Root, Scene,
    Texture,
};

/// A glTF document of `gltf-json`'s schema and its one buffer, which `encode` writes as a GLB
/// file: a 12-byte header, the JSON chunk padded with spaces, and the buffer's chunk padded with
/// zeros, each to 4 bytes.
#[derive(Debug)]
pub(crate) struct GlbBuilder {
    pub(crate) document: Root,
    buffer: Vec<u8>,
    /// Each texture pushed, by its image's URI and its wrap, so materials of one texture share
    /// its image, its sampler and itself.
    textures: Vec<PushedTexture>,
}

/// A texture the document holds, by what made it, with the image and the sampler it takes.
#[derive(Debug)]
struct PushedTexture {
    uri: String,
    wrap: [WrappingMode; 2],
    image: Index<Image>,
    sampler: Index<Sampler>,
    texture: Index<Texture>,
}

/// What an accessor's elements are, and the target of their view: none for data no mesh draws,
/// as an animation's keys and a skin's matrices.
#[derive(Debug, Clone, Copy)]
struct Elements {
    component: ComponentType,
    kind: Type,
    /// Whether an integer component reads as 0 to 1.
    normalized: bool,
    target: Option<Target>,
}

impl Elements {
    const VEC3: Self = Self::floats(Type::Vec3);
    const VEC2: Self = Self::floats(Type::Vec2);
    const VEC4: Self = Self::floats(Type::Vec4);
    const COLORS: Self = Self {
        component: ComponentType::U8,
        kind: Type::Vec4,
        normalized: true,
        target: Some(Target::ArrayBuffer),
    };
    const JOINTS: Self = Self {
        component: ComponentType::U16,
        kind: Type::Vec4,
        normalized: false,
        target: Some(Target::ArrayBuffer),
    };
    const INDICES: Self = Self {
        component: ComponentType::U32,
        kind: Type::Scalar,
        normalized: false,
        target: Some(Target::ElementArrayBuffer),
    };

    const fn floats(kind: Type) -> Self {
        Self {
            component: ComponentType::F32,
            kind,
            normalized: false,
            target: Some(Target::ArrayBuffer),
        }
    }

    const fn data(kind: Type) -> Self {
        Self {
            target: None,
            ..Self::floats(kind)
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
            textures: Vec::new(),
        }
    }

    /// The document's one scene.
    pub(crate) fn scene(&mut self) -> &mut Scene {
        &mut self.document.scenes[0]
    }

    /// Makes `child` a child of `parent`.
    pub(crate) fn child(&mut self, parent: Index<Node>, child: Index<Node>) {
        self.document.nodes[parent.value()]
            .children
            .get_or_insert_with(Vec::new)
            .push(child);
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
        let accessor = self.floats(vectors.as_flattened(), vectors.len(), Elements::VEC3);
        let bounds = &mut self.document.accessors[accessor.value()];
        bounds.min = Some(Json::convert(&min).expect("three floats are JSON"));
        bounds.max = Some(Json::convert(&max).expect("three floats are JSON"));
        accessor
    }

    /// An accessor of `vectors`.
    pub(crate) fn vectors(&mut self, vectors: &[[f32; 3]]) -> Index<Accessor> {
        self.floats(vectors.as_flattened(), vectors.len(), Elements::VEC3)
    }

    pub(crate) fn coordinates(&mut self, coordinates: &[[f32; 2]]) -> Index<Accessor> {
        self.floats(
            coordinates.as_flattened(),
            coordinates.len(),
            Elements::VEC2,
        )
    }

    /// An accessor of colors, each 4 bytes read as 0 to 1.
    pub(crate) fn colors(&mut self, colors: &[[u8; 4]]) -> Index<Accessor> {
        self.accessor(colors.iter().copied(), colors.len(), Elements::COLORS)
    }

    /// An accessor of each vertex's joints, four indices of a skin's.
    pub(crate) fn joints(&mut self, joints: &[[u16; 4]]) -> Index<Accessor> {
        let bytes = joints
            .as_flattened()
            .iter()
            .map(|joint| joint.to_le_bytes());
        self.accessor(bytes, joints.len(), Elements::JOINTS)
    }

    /// An accessor of each vertex's weights, one for each of its joints.
    pub(crate) fn weights(&mut self, weights: &[[f32; 4]]) -> Index<Accessor> {
        self.floats(weights.as_flattened(), weights.len(), Elements::VEC4)
    }

    /// An accessor of an animation's key times, in seconds, with their bounds, as glTF requires
    /// of a sampler's input.
    pub(crate) fn times(&mut self, times: &[f32]) -> Index<Accessor> {
        let accessor = self.floats(times, times.len(), Elements::data(Type::Scalar));
        let bounds = &mut self.document.accessors[accessor.value()];
        let [min, max] = [times.first(), times.last()].map(|time| {
            let time = *time.expect("a sampler of a key at least");
            Some(Json::convert(&[time]).expect("a float is JSON"))
        });
        (bounds.min, bounds.max) = (min, max);
        accessor
    }

    /// An accessor of an animation's moves, each a vector.
    pub(crate) fn moves(&mut self, moves: &[[f32; 3]]) -> Index<Accessor> {
        self.floats(
            moves.as_flattened(),
            moves.len(),
            Elements::data(Type::Vec3),
        )
    }

    /// An accessor of an animation's turns, each a quaternion `x, y, z, w`.
    pub(crate) fn turns(&mut self, turns: &[[f32; 4]]) -> Index<Accessor> {
        self.floats(
            turns.as_flattened(),
            turns.len(),
            Elements::data(Type::Vec4),
        )
    }

    /// An accessor of matrices, each its columns in order, as a skin's inverse bind matrices.
    pub(crate) fn matrices(&mut self, matrices: &[[f32; 16]]) -> Index<Accessor> {
        self.floats(
            matrices.as_flattened(),
            matrices.len(),
            Elements::data(Type::Mat4),
        )
    }

    pub(crate) fn indices(&mut self, indices: &[u32]) -> Index<Accessor> {
        let bytes = indices.iter().map(|index| index.to_le_bytes());
        self.accessor(bytes, indices.len(), Elements::INDICES)
    }

    /// A texture of the image at `uri`, wrapped as `wrap_s` and `wrap_t`: the one the document
    /// holds when another material made it, with the image of that URI and the sampler of that
    /// wrap it holds, each pushed once.
    pub(crate) fn texture(
        &mut self,
        uri: String,
        wrap_s: WrappingMode,
        wrap_t: WrappingMode,
    ) -> Index<Texture> {
        let wrap = [wrap_s, wrap_t];
        if let Some(pushed) = self
            .textures
            .iter()
            .find(|pushed| pushed.uri == uri && pushed.wrap == wrap)
        {
            return pushed.texture;
        }
        let image = self.textures.iter().find(|pushed| pushed.uri == uri);
        let image = image.map_or_else(
            || {
                self.document.push(Image {
                    buffer_view: None,
                    mime_type: None,
                    name: None,
                    uri: Some(uri.clone()),
                    extensions: None,
                    extras: Extras::default(),
                })
            },
            |pushed| pushed.image,
        );
        let sampler = self.textures.iter().find(|pushed| pushed.wrap == wrap);
        let sampler = sampler.map_or_else(
            || {
                self.document.push(Sampler {
                    wrap_s: Checked::Valid(wrap_s),
                    wrap_t: Checked::Valid(wrap_t),
                    ..Sampler::default()
                })
            },
            |pushed| pushed.sampler,
        );
        let texture = self.document.push(Texture {
            name: None,
            sampler: Some(sampler),
            source: image,
            extensions: None,
            extras: Extras::default(),
        });
        self.textures.push(PushedTexture {
            uri,
            wrap,
            image,
            sampler,
            texture,
        });
        texture
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
            self.use_extensions(specular.into_iter().chain(unlit).chain(others));
        }
        self.document.push(material)
    }

    /// `animation`'s index, with each extension it uses in `extensionsUsed`, in name order.
    pub(crate) fn animation(&mut self, animation: Animation) -> Index<Animation> {
        if let Some(extensions) = &animation.extensions {
            let others = extensions.others.keys().map(String::as_str);
            self.use_extensions(others);
        }
        self.document.push(animation)
    }

    /// Adds each of `extensions` that `extensionsUsed` lacks to it, in name order.
    fn use_extensions<'a>(&mut self, extensions: impl Iterator<Item = &'a str>) {
        let used = &mut self.document.extensions_used;
        for extension in extensions {
            if let Err(at) = used.binary_search_by(|known| known.as_str().cmp(extension)) {
                used.insert(at, extension.to_owned());
            }
        }
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
        let mut out = Vec::with_capacity(12 + chunks);
        out.extend(MAGIC.to_le_bytes());
        out.extend(VERSION.to_le_bytes());
        out.extend(word(12 + chunks));
        out.extend(word(json.len()));
        out.extend(JSON_CHUNK.to_le_bytes());
        out.extend(json);
        if !buffer.is_empty() {
            out.extend(word(buffer.len()));
            out.extend(BIN_CHUNK.to_le_bytes());
            out.extend(buffer);
        }
        debug_assert_eq!(out.len(), 12 + chunks, "the header counts every byte");
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

    fn floats(&mut self, floats: &[f32], count: usize, elements: Elements) -> Index<Accessor> {
        let bytes = floats.iter().map(|float| float.to_le_bytes());
        self.accessor(bytes, count, elements)
    }

    /// An accessor of `count` `elements`, whose bytes, `items` in turn, its own view holds, at the
    /// buffer's end aligned to 4, as every accessor's components need.
    fn accessor<const N: usize>(
        &mut self,
        items: impl ExactSizeIterator<Item = [u8; N]>,
        count: usize,
        elements: Elements,
    ) -> Index<Accessor> {
        let start = self.view_start();
        self.buffer.reserve(items.len() * N);
        for item in items {
            self.buffer.extend_from_slice(&item);
        }
        self.view(start, count, elements)
    }

    /// Where the next view starts: the buffer's end aligned to 4. The first adds the one buffer,
    /// whose length `encode` sets.
    fn view_start(&mut self) -> usize {
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
        self.buffer.len()
    }

    /// An accessor of `count` `elements`, whose view holds the buffer from `start` to its end.
    fn view(&mut self, start: usize, count: usize, elements: Elements) -> Index<Accessor> {
        let view = self.document.push(View {
            buffer: Index::new(0),
            byte_length: USize64::from(self.buffer.len() - start),
            byte_offset: Some(USize64::from(start)),
            byte_stride: None,
            name: None,
            target: elements.target.map(Checked::Valid),
            extensions: None,
            extras: Extras::default(),
        });
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

        pub(crate) fn joints_of(&self, accessor: Index<Accessor>) -> Vec<u16> {
            let (words, rest) = self.bytes_of(accessor).as_chunks::<2>();
            assert!(rest.is_empty());
            words.iter().copied().map(u16::from_le_bytes).collect()
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

    /// Materials of one texture share it: one of the same image and wrap is the same texture; one
    /// of another wrap is a texture of the same image with a sampler of its own; one of another
    /// image with that wrap shares its sampler. Three textures, two images, two samplers.
    #[test]
    fn each_image_sampler_and_texture_is_pushed_once() {
        use WrappingMode::{ClampToEdge, Repeat};
        let mut glb = GlbBuilder::new();
        let mut texture = |uri: &str, wrap_s, wrap_t| glb.texture(uri.to_owned(), wrap_s, wrap_t);
        let first = texture("a", Repeat, Repeat);
        let again = texture("a", Repeat, Repeat);
        let clamped = texture("a", ClampToEdge, Repeat);
        let other = texture("b", ClampToEdge, Repeat);
        assert_eq!(
            [first, again, clamped, other].map(|texture| texture.value()),
            [0, 0, 1, 2]
        );
        let document = &glb.document;
        assert_eq!(
            [
                document.images.len(),
                document.samplers.len(),
                document.textures.len()
            ],
            [2, 2, 3]
        );
        let parts = |at: usize| {
            let texture = &document.textures[at];
            [texture.source.value(), texture.sampler.unwrap().value()]
        };
        assert_eq!([parts(0), parts(1), parts(2)], [[0, 0], [0, 1], [1, 1]]);
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
