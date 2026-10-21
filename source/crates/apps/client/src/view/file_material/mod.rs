use bevy::app::{App, Plugin};
use bevy::asset::{Asset, Handle, embedded_asset};
use bevy::gltf::GltfMaterial;
use bevy::material::AlphaMode;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
    MaterialPlugin, StandardMaterial,
};
use bevy::reflect::Reflect;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use campfire_client_files::{Blend, MaterialExtras, TeamPaint};

use crate::view::match_colors::MatchColors;

/// The material of a model's glTF material: its standard material, as Bevy draws the glTF
/// material, drawn with its extras' blend, depth write, alpha test and depth bias, which the
/// standard material alone cannot all say, and painted by its mesh's tag, its unit's team's
/// color and its tint (02-engine-core.md, Team colors in the one material).
pub(crate) type FileMaterial = ExtendedMaterial<StandardMaterial, FileBlend>;

/// What a material's extras say past the standard material: how its color meets what is behind
/// it, whether it writes depth, the alpha below which it is not drawn, 0 for none, how its team
/// colors it, and the match's colors its mesh's tag names.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, PartialEq)]
#[bind_group_data(FileBlendKey)]
pub(crate) struct FileBlend {
    #[uniform(100)]
    pub(crate) cutoff: f32,
    /// `TINT` and `HUE_MASK`, as its extras' team paint says.
    #[uniform(100)]
    pub(crate) team: u32,
    #[storage(101, read_only)]
    pub(crate) colors: Handle<ShaderBuffer>,
    pub(crate) factors: Factors,
    pub(crate) depth_write: bool,
}

/// How a file material's color meets what is behind it, as its blend factors say.
#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Factors {
    /// It replaces what is behind.
    Replace,
    /// Its color by its alpha, and what is behind by one less it.
    Alpha,
    /// Its color plus what is behind.
    Add,
    /// What is behind times its color.
    Multiply,
    /// Its color plus what is behind times one less its color.
    Screen,
}

/// The pipeline's part of a file material: its factors and depth write.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileBlendKey {
    factors: Factors,
    depth_write: bool,
}

/// The shader of every file material, embedded in the client by `FileMaterialPlugin`, from this
/// module's directory.
const SHADER: &str = "embedded://campfire_client/view/file_material/file_material.wesl";

/// The bits of a file material's team paint, as its shader reads them.
const TINT: u32 = 1;
const HUE_MASK: u32 = 2;

/// The file materials: their shader, embedded, and their material's plugin.
#[derive(Debug)]
pub(crate) struct FileMaterialPlugin;

impl Plugin for FileMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "file_material.wesl");
        app.add_plugins(MaterialPlugin::<FileMaterial>::default());
    }
}

impl FileBlend {
    /// Whether a ray passes through it, as Zero Hour's pick passes a mesh whose shader blends with
    /// no alpha test, its headlight beams (`MeshClass::Cast_Ray`, `Is_Alpha`).
    pub(crate) fn lets_rays_through(&self) -> bool {
        self.factors != Factors::Replace && self.cutoff == 0.0
    }

    /// The extras a glTF material of none draws by, as Bevy draws its alpha mode: opaque or
    /// masked, writing depth, or mixed by its alpha, writing none, and painted by no team.
    pub(crate) const fn plain(gltf: &GltfMaterial) -> MaterialExtras {
        let (blend, depth_write) = match gltf.alpha_mode {
            AlphaMode::Opaque => (Blend::Opaque, true),
            AlphaMode::Mask(cutoff) => (Blend::Mask { cutoff }, true),
            _ => (Blend::Alpha, false),
        };
        MaterialExtras {
            blend,
            depth_write,
            depth_bias: 0,
            team: TeamPaint::NONE,
        }
    }

    /// The material of the glTF material `gltf` with `extras`: the standard material Bevy draws
    /// `gltf` with, its depth bias the extras', in the render phase their blend draws in; and the
    /// extras' blend and team paint, over the match's colors.
    pub(crate) fn material(gltf: &GltfMaterial, extras: MaterialExtras) -> FileMaterial {
        let (alpha_mode, factors, cutoff) = match extras.blend {
            Blend::Opaque => (AlphaMode::Opaque, Factors::Replace, 0.0),
            Blend::Mask { cutoff } => (AlphaMode::Mask(cutoff), Factors::Replace, cutoff),
            Blend::Alpha => (AlphaMode::Blend, Factors::Alpha, 0.0),
            Blend::AlphaMask { cutoff } => (AlphaMode::Blend, Factors::Alpha, cutoff),
            Blend::Add => (AlphaMode::Blend, Factors::Add, 0.0),
            Blend::Multiply => (AlphaMode::Blend, Factors::Multiply, 0.0),
            Blend::Screen => (AlphaMode::Blend, Factors::Screen, 0.0),
        };
        ExtendedMaterial {
            base: StandardMaterial {
                depth_bias: extras.depth_bias as f32,
                alpha_mode,
                ..Self::standard(gltf)
            },
            extension: Self {
                cutoff,
                team: (u32::from(extras.team.tint) * TINT)
                    | (u32::from(extras.team.hue_mask) * HUE_MASK),
                colors: MatchColors::BUFFER,
                factors,
                depth_write: extras.depth_write,
            },
        }
    }

    /// The standard material of `gltf`, as Bevy's glTF loader draws it: of each field
    /// `bevy_pbr`'s private `standard_material_from_gltf_material` takes in a build of the
    /// features the client's Bevy holds.
    fn standard(gltf: &GltfMaterial) -> StandardMaterial {
        StandardMaterial {
            base_color: gltf.base_color,
            base_color_channel: gltf.base_color_channel.clone(),
            base_color_texture: gltf.base_color_texture.clone(),
            emissive: gltf.emissive,
            emissive_channel: gltf.emissive_channel.clone(),
            emissive_texture: gltf.emissive_texture.clone(),
            perceptual_roughness: gltf.perceptual_roughness,
            metallic: gltf.metallic,
            metallic_roughness_channel: gltf.metallic_roughness_channel.clone(),
            metallic_roughness_texture: gltf.metallic_roughness_texture.clone(),
            reflectance: gltf.reflectance,
            specular_tint: gltf.specular_tint,
            specular_transmission: gltf.specular_transmission,
            thickness: gltf.thickness,
            ior: gltf.ior,
            attenuation_distance: gltf.attenuation_distance,
            attenuation_color: gltf.attenuation_color,
            normal_map_channel: gltf.normal_map_channel.clone(),
            normal_map_texture: gltf.normal_map_texture.clone(),
            occlusion_channel: gltf.occlusion_channel.clone(),
            occlusion_texture: gltf.occlusion_texture.clone(),
            clearcoat: gltf.clearcoat,
            clearcoat_perceptual_roughness: gltf.clearcoat_perceptual_roughness,
            anisotropy_strength: gltf.anisotropy_strength,
            anisotropy_rotation: gltf.anisotropy_rotation,
            double_sided: gltf.double_sided,
            cull_mode: gltf.cull_mode,
            unlit: gltf.unlit,
            alpha_mode: gltf.alpha_mode,
            uv_transform: gltf.uv_transform,
            ..StandardMaterial::default()
        }
    }
}

impl Factors {
    /// The blend of its factors on the color, and the usual blend of alpha; none for replace.
    pub(crate) fn blend(self) -> Option<BlendState> {
        let color = |src_factor, dst_factor| BlendComponent {
            src_factor,
            dst_factor,
            operation: BlendOperation::Add,
        };
        let color = match self {
            Self::Replace => return None,
            Self::Alpha => color(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
            Self::Add => color(BlendFactor::One, BlendFactor::One),
            Self::Multiply => color(BlendFactor::Zero, BlendFactor::Src),
            Self::Screen => color(BlendFactor::One, BlendFactor::OneMinusSrc),
        };
        Some(BlendState {
            color,
            alpha: BlendComponent::OVER,
        })
    }
}

impl From<&FileBlend> for FileBlendKey {
    fn from(blend: &FileBlend) -> Self {
        Self {
            factors: blend.factors,
            depth_write: blend.depth_write,
        }
    }
}

impl MaterialExtension for FileBlend {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let FileBlendKey {
            factors,
            depth_write,
        } = key.bind_group_data;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(depth_write);
        }
        if let (Some(blend), Some(fragment)) = (factors.blend(), descriptor.fragment.as_mut()) {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(blend);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
