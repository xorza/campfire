use bevy::asset::{Asset, Handle};
use bevy::color::{Color, LinearRgba};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
    StandardMaterial,
};
use bevy::reflect::Reflect;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Face,
    RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use campfire_package::{Blend, MaterialFile};

/// A material file's material: the standard material of its colors and texture, drawn with the
/// file's blend, depth write and alpha test, which the standard material alone cannot all say.
pub(crate) type FileMaterial = ExtendedMaterial<StandardMaterial, FileBlend>;

/// What a material file says past the standard material: how its color meets what is behind it,
/// whether it writes depth, and the alpha below which it is not drawn, 0 for none.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, PartialEq)]
#[bind_group_data(FileBlendKey)]
pub(crate) struct FileBlend {
    #[uniform(100)]
    pub(crate) cutoff: f32,
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

/// The shader of every file material, embedded in the client.
const SHADER: &str = "embedded://campfire_client/view/file_material.wgsl";

impl FileBlend {
    /// The material of `file`, its texture `texture`: the standard material of the file's colors,
    /// its metallic and roughness factors, its reflectance half its specular factor, as Bevy's
    /// glTF loader reads `KHR_materials_specular`, lit or not as it says, and its depth bias, in
    /// the render phase its blend draws in; and the file's blend.
    pub(crate) fn material(file: &MaterialFile, texture: Option<Handle<Image>>) -> FileMaterial {
        let [r, g, b, a] = file.base_color;
        let [er, eg, eb] = file.emissive;
        let (alpha_mode, factors, cutoff) = match file.blend {
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
                base_color: Color::LinearRgba(LinearRgba::new(r, g, b, a)),
                base_color_texture: texture,
                emissive: LinearRgba::new(er, eg, eb, 1.0),
                metallic: file.metallic,
                perceptual_roughness: file.roughness,
                reflectance: file.specular * 0.5,
                unlit: file.unlit,
                depth_bias: file.depth_bias as f32,
                alpha_mode,
                double_sided: file.double_sided,
                cull_mode: (!file.double_sided).then_some(Face::Back),
                ..StandardMaterial::default()
            },
            extension: FileBlend {
                cutoff,
                factors,
                depth_write: file.depth_write,
            },
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
            Factors::Replace => return None,
            Factors::Alpha => color(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
            Factors::Add => color(BlendFactor::One, BlendFactor::One),
            Factors::Multiply => color(BlendFactor::Zero, BlendFactor::Src),
            Factors::Screen => color(BlendFactor::One, BlendFactor::OneMinusSrc),
        };
        Some(BlendState {
            color,
            alpha: BlendComponent::OVER,
        })
    }
}

impl From<&FileBlend> for FileBlendKey {
    fn from(blend: &FileBlend) -> FileBlendKey {
        FileBlendKey {
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
mod tests {
    use bevy::pbr::Material;

    use super::*;

    #[test]
    fn a_file_gives_its_phase_its_blend_its_depth_write_and_its_test() {
        let file = |blend, depth_write| MaterialFile {
            base_color: [1.0, 0.5, 0.25, 0.75],
            texture: None,
            emissive: [0.5, 0.0, 0.0],
            metallic: 0.0,
            roughness: 1.0,
            specular: 0.0,
            unlit: false,
            blend,
            depth_write,
            depth_bias: 0,
            double_sided: false,
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
            let material = FileBlend::material(&file(blend, depth_write), None);
            assert_eq!(material.base.alpha_mode, alpha_mode, "{blend:?}");
            assert_eq!(
                material.extension,
                FileBlend {
                    cutoff: test,
                    factors,
                    depth_write,
                }
            );
            assert_eq!(
                material.base.base_color,
                Color::LinearRgba(LinearRgba::new(1.0, 0.5, 0.25, 0.75))
            );
            assert_eq!(material.base.emissive, LinearRgba::new(0.5, 0.0, 0.0, 1.0));
            assert_eq!(material.base.cull_mode, Some(Face::Back));
        }
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
        // A two-sided file culls no face.
        let both = MaterialFile {
            double_sided: true,
            ..file(Blend::Opaque, true)
        };
        assert_eq!(FileBlend::material(&both, None).base.cull_mode, None);
        // The file's lighting: its metallic and roughness as they are; its reflectance half its
        // specular, 0.6 / 2 = 0.3, and 0 for the import's 0; lit or not as it says.
        let shiny = MaterialFile {
            metallic: 0.25,
            roughness: 0.5,
            specular: 0.6,
            unlit: true,
            ..file(Blend::Opaque, true)
        };
        let lighting = |material: &FileMaterial| {
            let base = &material.base;
            (
                base.metallic,
                base.perceptual_roughness,
                base.reflectance,
                base.unlit,
            )
        };
        assert_eq!(
            lighting(&FileBlend::material(&shiny, None)),
            (0.25, 0.5, 0.3, true)
        );
        let matte = FileBlend::material(&file(Blend::Opaque, true), None);
        assert_eq!(lighting(&matte), (0.0, 1.0, 0.0, false));
        // Two blended layers of one mesh, 1 and 2 over the pass under them: Bevy sorts a blended
        // item by its distance plus its depth bias, so of one mesh, one distance `d`, the layer
        // of `d + 2` draws after that of `d + 1`, and its pipeline's constant depth bias, 2 to 1,
        // passes the depth test over it.
        let layer = |depth_bias| {
            let file = MaterialFile {
                depth_bias,
                ..file(Blend::Add, false)
            };
            Material::depth_bias(&FileBlend::material(&file, None))
        };
        assert_eq!((layer(0), layer(1), layer(2)), (0.0, 1.0, 2.0));
    }
}
