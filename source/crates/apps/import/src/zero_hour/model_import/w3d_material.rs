use campfire_client_files::{Blend, MaterialFile, MaterialTexture};
use campfire_values::PackagePath;

use crate::gltf::glb_builder::{CLAMP_TO_EDGE, REPEAT};
use crate::gltf::gltf_document::{Material, MaterialExtensions, Pbr, Specular, Unlit};
use crate::texture::srgb;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ModelError;
use crate::zero_hour::format::w3d::w3d_mesh::{
    CUTOFF, DETAIL_ADD, DETAIL_DISABLE, DETAIL_SCALE, NullTexture, ONE, ONE_MINUS_SRC_ALPHA,
    ONE_MINUS_SRC_COLOR, SRC_ALPHA, SRC_COLOR, Shader, VertexMaterial, ZERO,
};
use crate::zero_hour::game_path;

/// What makes one material of a model: its surface, what colors it, its two-sidedness and its
/// depth bias, its layer's order over the pass it covers, so equal ones share a material.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MaterialKey {
    pub(super) surface: Surface,
    pub(super) source: Source,
    pub(super) two_sided: bool,
    pub(super) depth_bias: i32,
}

/// What colors a material: a pass's shader and vertex material, or a detail stage's function,
/// whose texture is drawn alone over the pass by its blend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Source {
    Pass {
        shader: Shader,
        material: VertexMaterial,
    },
    Detail {
        blend: Blend,
    },
}

/// What a material's triangles are drawn with: no texture; a texture's path and wraps; or a
/// texture no archive holds, which the game draws as `MissingTexture`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Surface {
    Untextured,
    Texture { path: ArchivePath, flags: u16 },
    Missing,
}

/// A key's material: its glTF material, of no texture yet, the texture it samples, and the
/// material file beside it when glTF's material says less than its shader.
#[derive(Debug)]
pub(super) struct KeyMaterial {
    pub(super) gltf: Material,
    pub(super) texture: Option<KeyTexture>,
    pub(super) file: Option<MaterialFile>,
}

/// A texture a glTF material samples: its URI from the model's file, and its wraps along `s` and
/// `t`.
#[derive(Debug)]
pub(super) struct KeyTexture {
    pub(super) uri: String,
    pub(super) wrap_s: u32,
    pub(super) wrap_t: u32,
}

/// Every material's metallic, roughness and specular factors: a W3D material is lit by its
/// colors alone, with no reflection, as no shader of the game turns its specular on.
const METALLIC: f32 = 0.0;
const ROUGHNESS: f32 = 1.0;
const SPECULAR: f32 = 0.0;

/// `MissingTexture`'s every texel, `0x7FFF00FF`, as glTF's linear factors: red and blue full, no
/// green, and an alpha of 127 of 255, which no sRGB curve bends.
const MISSING: [f32; 4] = [1.0, 0.0, 1.0, 127.0 / 255.0];

/// A texture's clamp flags, `W3DTEXTURE_CLAMP_U` and `_V`.
const CLAMP_U: u16 = 0x0008;
const CLAMP_V: u16 = 0x0010;

impl MaterialKey {
    /// Its material, of the name `name`, of the model whose file is at `model`. It is rough, not
    /// metallic, and of no specular, as no shader of the game turns its specular on; unlit,
    /// white and of no emissive light, when its shader draws its texture alone; of
    /// `MissingTexture`'s color times its own in place of a texture no archive holds (design 12,
    /// D13); and, for a triangle of no texture under a shader that textures, of what its shader
    /// draws of the null vector. A material file goes beside it if its shader is more than
    /// glTF's material says.
    pub(super) fn convert(
        &self,
        name: &str,
        model: &PackagePath,
    ) -> Result<KeyMaterial, ModelError> {
        let (blend, depth_write, colors) = match self.source {
            Source::Pass { shader, material } => {
                let blend = pass_blend(shader).ok_or(ModelError::Shader {
                    src: shader.src_blend,
                    dest: shader.dest_blend,
                })?;
                (
                    blend,
                    shader.depth_write,
                    (!shader.unlit()).then_some(material),
                )
            }
            Source::Detail { blend } => (blend, false, None),
        };
        let unlit = colors.is_none();
        let (lit_color, lit_emissive) = match colors {
            None => ([1.0; 4], [0.0; 3]),
            Some(material) => {
                let [r, g, b] = material.diffuse.map(linear);
                let emissive = material.emissive.map(linear);
                ([r, g, b, material.opacity], emissive)
            }
        };
        // A detail stage of no texture samples the null vector too, which its scale makes black
        // and its add leaves as it was.
        let null = match self.source {
            Source::Pass { shader, .. } => shader.null_texture(),
            Source::Detail { .. } => NullTexture::Clear,
        };
        let [r, g, b, _] = lit_color;
        let (base_color, emissive) = match (&self.surface, null) {
            (Surface::Missing, _) => (
                [0, 1, 2, 3].map(|at| lit_color[at] * MISSING[at]),
                lit_emissive,
            ),
            (Surface::Untextured, NullTexture::Clear) => ([0.0; 4], [0.0; 3]),
            (Surface::Untextured, NullTexture::ClearLight) => ([r, g, b, 0.0], lit_emissive),
            (Surface::Untextured, NullTexture::Light) | (Surface::Texture { .. }, _) => {
                (lit_color, lit_emissive)
            }
        };
        let texture = match &self.surface {
            Surface::Texture { path, flags } => Some(MaterialTexture {
                path: path.texture_path()?,
                clamp_u: flags & CLAMP_U != 0,
                clamp_v: flags & CLAMP_V != 0,
            }),
            Surface::Untextured | Surface::Missing => None,
        };
        let wrap = |clamp: bool| if clamp { CLAMP_TO_EDGE } else { REPEAT };
        let gltf_texture = texture.as_ref().map(|texture| KeyTexture {
            uri: game_path::relative_uri(model, &texture.path),
            wrap_s: wrap(texture.clamp_u),
            wrap_t: wrap(texture.clamp_v),
        });
        let (alpha_mode, alpha_cutoff, gltf_depth_write) = match blend {
            Blend::Opaque => ("OPAQUE", None, true),
            Blend::Mask { cutoff } => ("MASK", Some(cutoff), true),
            _ => ("BLEND", None, false),
        };
        let gltf = Material {
            name: name.to_owned(),
            pbr: Pbr {
                base_color,
                texture: None,
                metallic: METALLIC,
                roughness: ROUGHNESS,
            },
            emissive,
            alpha_mode,
            alpha_cutoff,
            double_sided: self.two_sided,
            extensions: MaterialExtensions {
                specular: Specular { factor: SPECULAR },
                unlit: unlit.then_some(Unlit {}),
            },
        };
        let says_all = matches!(blend, Blend::Opaque | Blend::Mask { .. } | Blend::Alpha)
            && gltf_depth_write == depth_write
            && self.depth_bias == 0;
        let file = (!says_all).then_some(MaterialFile {
            base_color,
            texture,
            emissive,
            metallic: METALLIC,
            roughness: ROUGHNESS,
            specular: SPECULAR,
            unlit,
            blend,
            depth_write,
            depth_bias: self.depth_bias,
            double_sided: self.two_sided,
        });
        Ok(KeyMaterial {
            gltf,
            texture: gltf_texture,
            file,
        })
    }
}

/// The blend of a pass of `shader`, as a material file names it; `None` for a pair of factors no
/// shipped shader uses.
pub(super) const fn pass_blend(shader: Shader) -> Option<Blend> {
    Some(
        match (shader.src_blend, shader.dest_blend, shader.alpha_test) {
            (ONE, ZERO, false) => Blend::Opaque,
            (ONE, ZERO, true) => Blend::Mask { cutoff: CUTOFF },
            (SRC_ALPHA, ONE_MINUS_SRC_ALPHA, false) => Blend::Alpha,
            (SRC_ALPHA, ONE_MINUS_SRC_ALPHA, true) => Blend::AlphaMask { cutoff: CUTOFF },
            (ONE, ONE, false) => Blend::Add,
            (ZERO, SRC_COLOR, false) => Blend::Multiply,
            (ONE, ONE_MINUS_SRC_COLOR, false) => Blend::Screen,
            _ => return None,
        },
    )
}

/// The blend the detail stage of `shader` draws by over its first: none for a function that
/// keeps the first stage alone, as with texturing off; a multiplying blend for scale and an
/// additive one for add, the detail's color drawn alone over the first stage's lit one, as the
/// game mixes them after the lighting (`ShaderClass::Apply`); `Err` with a function no shipped
/// shader uses.
pub(super) const fn detail_blend(shader: Shader) -> Result<Option<Blend>, u8> {
    match shader.detail_color {
        _ if !shader.texturing => Ok(None),
        DETAIL_DISABLE => Ok(None),
        DETAIL_SCALE => Ok(Some(Blend::Multiply)),
        DETAIL_ADD => Ok(Some(Blend::Add)),
        other => Err(other),
    }
}

/// An sRGB code as glTF's linear factor: its light, at most 2¹⁶ steps of 2⁻¹⁶.
#[expect(
    clippy::cast_precision_loss,
    reason = "a light is at most 2¹⁶, exact in f32"
)]
fn linear(code: u8) -> f32 {
    srgb::light(code) as f32 / 65_536.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shipped_blend_is_a_mode_and_another_is_none() {
        let shader = |src_blend, dest_blend, alpha_test| Shader {
            dest_blend,
            src_blend,
            alpha_test,
            ..Shader::DEFAULT
        };
        let blend = |src, dest, alpha_test| pass_blend(shader(src, dest, alpha_test));
        let cutoff = 96.0 / 255.0;
        assert_eq!(blend(ONE, ZERO, false), Some(Blend::Opaque));
        assert_eq!(blend(ONE, ZERO, true), Some(Blend::Mask { cutoff }));
        assert_eq!(
            blend(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, false),
            Some(Blend::Alpha)
        );
        assert_eq!(
            blend(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, true),
            Some(Blend::AlphaMask { cutoff })
        );
        assert_eq!(blend(ONE, ONE, false), Some(Blend::Add));
        assert_eq!(blend(ZERO, SRC_COLOR, false), Some(Blend::Multiply));
        assert_eq!(blend(ONE, ONE_MINUS_SRC_COLOR, false), Some(Blend::Screen));
        assert_eq!(blend(ONE, ONE, true), None);
        assert_eq!(blend(SRC_ALPHA, ONE, false), None);
    }
}
