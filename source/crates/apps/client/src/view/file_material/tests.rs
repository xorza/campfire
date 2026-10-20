use bevy::DefaultPlugins;
use bevy::app::{App, PluginGroup};
use bevy::asset::{AssetId, Assets, uuid::Uuid};
use bevy::color::{Color, LinearRgba};
use bevy::log::LogPlugin;
use bevy::pbr::Material;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::Face;
use bevy::render::settings::WgpuSettings;
use bevy::shader::{Shader, ShaderCache, ShaderCacheError, ShaderCacheSource, ShaderDefVal};
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;
use naga::Module;
use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use super::*;

#[test]
fn extras_give_their_phase_blend_depth_write_test_and_bias_over_the_gltf_material() {
    // A glTF material of a color, an emission, its factors and one face, as Bevy reads it.
    let gltf = GltfMaterial {
        base_color: Color::LinearRgba(LinearRgba::new(1.0, 0.5, 0.25, 0.75)),
        emissive: LinearRgba::new(0.5, 0.0, 0.0, 1.0),
        metallic: 0.25,
        perceptual_roughness: 0.5,
        reflectance: 0.3,
        unlit: true,
        cull_mode: Some(Face::Back),
        ..GltfMaterial::default()
    };
    let extras = |blend, depth_write, depth_bias| MaterialExtras {
        blend,
        depth_write,
        depth_bias,
        team: TeamPaint::NONE,
    };
    let cutoff = 96.0 / 255.0;
    let cases = [
        (
            Blend::Opaque,
            true,
            AlphaMode::Opaque,
            Factors::Replace,
            0.0,
        ),
        (
            Blend::Mask { cutoff },
            true,
            AlphaMode::Mask(cutoff),
            Factors::Replace,
            cutoff,
        ),
        (Blend::Alpha, false, AlphaMode::Blend, Factors::Alpha, 0.0),
        (
            Blend::AlphaMask { cutoff },
            true,
            AlphaMode::Blend,
            Factors::Alpha,
            cutoff,
        ),
        (Blend::Add, false, AlphaMode::Blend, Factors::Add, 0.0),
        (
            Blend::Multiply,
            false,
            AlphaMode::Blend,
            Factors::Multiply,
            0.0,
        ),
        (Blend::Screen, false, AlphaMode::Blend, Factors::Screen, 0.0),
    ];
    for (blend, depth_write, alpha_mode, factors, test) in cases {
        let material = FileBlend::material(&gltf, extras(blend, depth_write, 0));
        assert_eq!(material.base.alpha_mode, alpha_mode, "{blend:?}");
        assert_eq!(
            material.extension,
            FileBlend {
                cutoff: test,
                team: 0,
                colors: MatchColors::BUFFER,
                factors,
                depth_write,
            }
        );
        // The rest is the glTF material's, as Bevy draws it.
        let base = &material.base;
        assert_eq!(base.base_color, gltf.base_color);
        assert_eq!(base.emissive, gltf.emissive);
        assert_eq!(base.cull_mode, Some(Face::Back));
        assert_eq!(
            (
                base.metallic,
                base.perceptual_roughness,
                base.reflectance,
                base.unlit
            ),
            (0.25, 0.5, 0.3, true)
        );
    }
    // A team's paint is its bits: a tint 1, a hue mask 2, both 3.
    let painted = |tint, hue_mask| {
        let extras = MaterialExtras {
            team: TeamPaint { tint, hue_mask },
            ..extras(Blend::Opaque, true, 0)
        };
        FileBlend::material(&gltf, extras).extension.team
    };
    assert_eq!(
        [
            painted(true, false),
            painted(false, true),
            painted(true, true)
        ],
        [1, 2, 3]
    );
    // A material of no extras draws as Bevy draws its alpha mode.
    let plain = |alpha_mode| {
        FileBlend::plain(&GltfMaterial {
            alpha_mode,
            ..gltf.clone()
        })
    };
    assert_eq!(plain(AlphaMode::Opaque), extras(Blend::Opaque, true, 0));
    assert_eq!(
        plain(AlphaMode::Mask(cutoff)),
        extras(Blend::Mask { cutoff }, true, 0)
    );
    assert_eq!(plain(AlphaMode::Blend), extras(Blend::Alpha, false, 0));
    // Each blend's factors on the color: W3D's own, `src · s + dst · d`.
    let color = |factors: Factors| {
        factors
            .blend()
            .map(|blend| (blend.color.src_factor, blend.color.dst_factor))
    };
    assert_eq!(color(Factors::Replace), None);
    assert_eq!(
        color(Factors::Alpha),
        Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha))
    );
    assert_eq!(
        color(Factors::Add),
        Some((BlendFactor::One, BlendFactor::One))
    );
    assert_eq!(
        color(Factors::Multiply),
        Some((BlendFactor::Zero, BlendFactor::Src))
    );
    assert_eq!(
        color(Factors::Screen),
        Some((BlendFactor::One, BlendFactor::OneMinusSrc))
    );
    // Two blended layers of one mesh, 1 and 2 over the pass under them: Bevy sorts a blended
    // item by its distance plus its depth bias, so of one mesh, one distance `d`, the layer
    // of `d + 2` draws after that of `d + 1`, and its pipeline's constant depth bias, 2 to 1,
    // passes the depth test over it.
    let layer = |depth_bias| {
        Material::depth_bias(&FileBlend::material(
            &gltf,
            extras(Blend::Add, false, depth_bias),
        ))
    };
    assert_eq!((layer(0), layer(1), layer(2)), (0.0, 1.0, 2.0));
}

/// An app of the default plugins with no render backend, whose plugins load Bevy's shader
/// modules on its IO pool, and a cache of the modules loaded, as the renderer's holds them,
/// whose WGSL `naga` validates, as wgpu does.
struct Shaders {
    app: App,
    cache: ShaderCache<Module, ()>,
}

impl Shaders {
    /// The most frames the app runs for Bevy's shader modules to load.
    const LOAD_FRAMES: usize = 1000;

    fn new() -> Self {
        let mut app = App::new();
        let render = RenderPlugin {
            render_creation: WgpuSettings {
                backends: None,
                ..WgpuSettings::default()
            }
            .into(),
            ..RenderPlugin::default()
        };
        let window = WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..WindowPlugin::default()
        };
        app.add_plugins(
            DefaultPlugins
                .set(render)
                .set(window)
                .disable::<WinitPlugin>()
                .disable::<LogPlugin>(),
        );
        app.finish();
        app.cleanup();
        let cache = ShaderCache::new((), |(), source, _| {
            let ShaderCacheSource::Wgsl(text) = source else {
                return Err(ShaderCacheError::CreateShaderModule("no WGSL".to_owned()));
            };
            let failed = ShaderCacheError::CreateShaderModule;
            let module =
                wgsl::parse_str(&text).map_err(|error| failed(error.emit_to_string(&text)))?;
            Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&module)
                .map_err(|error| failed(error.emit_to_string(&text)))?;
            Ok(module)
        });
        Self { app, cache }
    }

    /// Composes `source` as the material's shader, of the id `at`, with `defs`: while one of
    /// its imports has not loaded, the app runs a frame more and the cache takes the
    /// modules loaded since.
    fn compose(
        &mut self,
        at: u128,
        source: &str,
        defs: &[ShaderDefVal],
    ) -> Result<(), ShaderCacheError> {
        let id = AssetId::Uuid {
            uuid: Uuid::from_u128(at),
        };
        self.cache
            .set_shader(id, Shader::from_wesl(source.to_owned(), SHADER));
        for _ in 0..Self::LOAD_FRAMES {
            match self.cache.get(0, id, defs) {
                Err(ShaderCacheError::ShaderImportNotYetAvailable) => {
                    self.app.update();
                    for (loaded, shader) in self.app.world().resource::<Assets<Shader>>().iter() {
                        self.cache.set_shader(loaded, shader.clone());
                    }
                }
                composed => return composed.map(drop),
            }
        }
        panic!(
            "Bevy's shader modules load within {} frames",
            Self::LOAD_FRAMES
        );
    }
}

/// Bevy composes the material's shader in its main pass alone: its prepass and its shadows draw
/// by the standard material's prepass shader, as the material names none of its own.
#[test]
fn the_shader_composes_and_validates_opaque_and_where_it_may_discard() {
    // The definitions of a mesh's pipeline of positions, normals and one set of UVs, seen in
    // perspective, and the material's bind group, 3, as Bevy's mesh pipeline gives them; a mask
    // and a blend add that a fragment may discard.
    let flags = [
        "MESH_PIPELINE",
        "VERTEX_OUTPUT_INSTANCE_INDEX",
        "VERTEX_POSITIONS",
        "VERTEX_NORMALS",
        "VERTEX_UVS",
        "VERTEX_UVS_A",
        "VIEW_PROJECTION_PERSPECTIVE",
    ];
    let mut opaque: Vec<ShaderDefVal> = flags.into_iter().map(ShaderDefVal::from).collect();
    opaque.push(ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3));
    let mut discards = opaque.clone();
    discards.push("MAY_DISCARD".into());
    let mut shaders = Shaders::new();
    let shader = include_str!("file_material.wesl");
    for (at, defs) in [(1, &opaque), (2, &discards)] {
        let composed = shaders.compose(at, shader, defs);
        assert!(composed.is_ok(), "{defs:?}: {composed:?}");
    }
    // A field the material's uniform lacks fails it, as wgpu's validation does.
    let misspelled = shader.replace("file_blend.cutoff", "file_blend.cut");
    assert!(matches!(
        shaders.compose(3, &misspelled, &opaque),
        Err(ShaderCacheError::CreateShaderModule(_))
    ));
}
