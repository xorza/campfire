use campfire_client_files::{Blend, MaterialExtras, TeamPaint};
use campfire_codec::Json;
use campfire_values::PackagePath;
use gltf_json::Material;
use gltf_json::extensions::material::{
    Material as MaterialExtensions, Specular, SpecularFactor, Unlit,
};
use gltf_json::material::{
    AlphaCutoff, AlphaMode, EmissiveFactor, PbrBaseColorFactor, PbrMetallicRoughness,
    StrengthFactor,
};
use gltf_json::texture::WrappingMode;
use gltf_json::validation::Checked;

use crate::texture::srgb;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ModelError;
use crate::zero_hour::format::w3d::w3d_mesh::{
    CUTOFF, DETAIL_ADD, DETAIL_DISABLE, DETAIL_SCALE, NullTexture, ONE, ONE_MINUS_SRC_ALPHA,
    ONE_MINUS_SRC_COLOR, SRC_ALPHA, SRC_COLOR, Shader, VertexMaterial, ZERO,
};
use crate::zero_hour::game_path;

/// What makes one material of a model: its surface, what colors it, its two-sidedness, its
/// depth bias, its layer's order over the pass it covers, and how its unit's team colors it, so
/// equal ones share a material.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MaterialKey {
    pub(super) surface: Surface,
    pub(super) source: Source,
    pub(super) two_sided: bool,
    pub(super) depth_bias: i32,
    pub(super) team: TeamPaint,
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

/// A key's material: its glTF material, of no texture yet, and the texture it samples.
#[derive(Debug)]
pub(super) struct KeyMaterial {
    pub(super) gltf: Material,
    pub(super) texture: Option<KeyTexture>,
}

/// A texture a glTF material samples: its URI from the model's file, and its wraps along `s` and
/// `t`.
#[derive(Debug)]
pub(super) struct KeyTexture {
    pub(super) uri: String,
    pub(super) wrap_s: WrappingMode,
    pub(super) wrap_t: WrappingMode,
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
    /// metallic, and of no specular, as no shader of the game turns its specular on; unlit, white
    /// and of no emissive light, when its shader draws its texture alone; of `MissingTexture`'s
    /// color times its own in place of a texture no archive holds (09-zero-hour.md, A W3D material
    /// becomes the one material); and, for a triangle of no texture under a shader that textures,
    /// of what its shader draws of the null vector. Its extras go in its extension when its shader
    /// is more than glTF's material says (02-engine-core.md, Material extras in a glTF extension).
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
                    (!shader.is_unlit()).then_some(material),
                )
            }
            Source::Detail { blend } => (blend, false, None),
        };
        let blend = if self.team.hue_mask {
            hue_mask_blend(blend)
        } else {
            blend
        };
        let unlit = colors.is_none();
        let (lit_color, lit_emissive) = match colors {
            None => ([1.0; 4], [0.0; 3]),
            Some(material) => {
                // A tinted material's diffuse is its team's color, as the game sets it, which it
                // takes as white before.
                let [r, g, b] = if self.team.tint {
                    [1.0; 3]
                } else {
                    material.diffuse.map(linear)
                };
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
            Surface::Texture { path, flags } => {
                let held = |flag: u16| flags & flag != 0;
                let wrap = |flag| {
                    if held(flag) {
                        WrappingMode::ClampToEdge
                    } else {
                        WrappingMode::Repeat
                    }
                };
                Some(KeyTexture {
                    uri: game_path::relative_uri(model, &path.texture_path()?),
                    wrap_s: wrap(CLAMP_U),
                    wrap_t: wrap(CLAMP_V),
                })
            }
            Surface::Untextured | Surface::Missing => None,
        };
        let (alpha_mode, alpha_cutoff, gltf_depth_write) = match blend {
            Blend::Opaque => (AlphaMode::Opaque, None, true),
            Blend::Mask { cutoff } => (AlphaMode::Mask, Some(AlphaCutoff(cutoff)), true),
            _ => (AlphaMode::Blend, None, false),
        };
        let says_all = matches!(blend, Blend::Opaque | Blend::Mask { .. } | Blend::Alpha)
            && gltf_depth_write == depth_write
            && self.depth_bias == 0
            && self.team.is_none();
        let extras = (!says_all).then_some(MaterialExtras {
            blend,
            depth_write,
            depth_bias: self.depth_bias,
            team: self.team,
        });
        let others = extras
            .map(|extras| {
                let value = Json::convert(&extras).expect("a material's extras are a JSON object");
                (MaterialExtras::EXTENSION.to_owned(), value)
            })
            .into_iter()
            .collect();
        let gltf = Material {
            name: Some(name.to_owned()),
            pbr_metallic_roughness: PbrMetallicRoughness {
                base_color_factor: PbrBaseColorFactor(base_color),
                metallic_factor: StrengthFactor(METALLIC),
                roughness_factor: StrengthFactor(ROUGHNESS),
                ..PbrMetallicRoughness::default()
            },
            emissive_factor: EmissiveFactor(emissive),
            alpha_mode: Checked::Valid(alpha_mode),
            alpha_cutoff,
            double_sided: self.two_sided,
            extensions: Some(MaterialExtensions {
                specular: Some(Specular {
                    specular_factor: SpecularFactor(SPECULAR),
                    ..Specular::default()
                }),
                unlit: unlit.then(Unlit::default),
                others,
                ..MaterialExtensions::default()
            }),
            ..Material::default()
        };
        Ok(KeyMaterial { gltf, texture })
    }
}

/// How a team colors a material of the mesh `mesh` of a texture of the name `texture`, as
/// `W3DAssetManager::Recolor_Mesh` recolors it: a pass's vertex materials take the team's color
/// when the mesh's name, past its container's and its `.`, starts with `HOUSECOLOR`, of either
/// case, and a texture whose name starts with `ZHCA` is the team's hue where its alpha is below
/// one. The game's palette textures, of `ZHCD`, no shipped model uses.
pub(super) fn team_paint(mesh: &str, pass: bool, texture: Option<&str>) -> TeamPaint {
    let own = mesh.split_once('.').map_or(mesh, |(_, own)| own);
    let starts = |name: &str, prefix: &str| {
        name.get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    };
    TeamPaint {
        tint: pass && starts(own, "HOUSECOLOR"),
        hue_mask: texture.is_some_and(|texture| starts(texture, "ZHCA")),
    }
}

/// The blend of a pass of `shader`, as the material extras name it; `None` for a pair of factors no
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

/// The blend of a material of a hue-mask texture, whose alpha is its team's mask and no
/// transparency, as the game's recolor makes the texture opaque (`W3DAssetManager`'s
/// `remapAlphaTexture32Bit`): a mask's tests nothing, and an alpha mask's blends by the material's
/// opacity alone (09-zero-hour.md, A hue mask's alpha is no transparency).
const fn hue_mask_blend(blend: Blend) -> Blend {
    match blend {
        Blend::Mask { .. } => Blend::Opaque,
        Blend::AlphaMask { .. } => Blend::Alpha,
        other => other,
    }
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
    fn a_house_color_mesh_tints_and_a_zhca_texture_masks_its_team_s_hue() {
        let paint = |mesh, pass, texture| {
            let paint = team_paint(mesh, pass, texture);
            (paint.tint, paint.hue_mask)
        };
        // The mesh's own name past its container's, of either case; a detail stage, of no vertex
        // material, is not tinted; a texture of `ZHCA`, of either case, masks; `ZHCD`, a name of
        // `HOUSECOLOR` in the container's part, or none, does neither.
        assert_eq!(paint("AVTANK.HouseColor01", true, None), (true, false));
        assert_eq!(
            paint("HOUSECOLOR", true, Some("zhca_tank.tga")),
            (true, true)
        );
        assert_eq!(paint("AVTANK.HOUSECOLOR", false, None), (false, false));
        assert_eq!(
            paint("HOUSECOLOR.HULL", true, Some("ZHCD_X.tga")),
            (false, false)
        );
        assert_eq!(
            paint("AVTANK.HULL", true, Some("ZHCA_X.tga")),
            (false, true)
        );
        assert_eq!(paint("AVTANK.HULL", true, Some("ZH")), (false, false));
    }

    #[test]
    fn a_tinted_material_is_white_under_its_team_s_color_and_says_so_in_its_extras() {
        // A lit pass of a diffuse of (255, 128, 0), on a texture no archive holds: its color is the
        // diffuse times `MissingTexture`'s, (1, 0, 0) of red and blue full and no green; tinted,
        // white times it, (1, 0, 1), and its extras name the tint.
        let key = |team| MaterialKey {
            surface: Surface::Missing,
            source: Source::Pass {
                shader: Shader::DEFAULT,
                material: VertexMaterial {
                    diffuse: [255, 128, 0],
                    ..VertexMaterial::DEFAULT
                },
            },
            two_sided: false,
            depth_bias: 0,
            team,
        };
        let model = PackagePath::parse("client/models/tank.glb").unwrap();
        let tint = TeamPaint {
            tint: true,
            hue_mask: false,
        };
        let plain = key(TeamPaint::NONE).convert("plain", &model).unwrap().gltf;
        let tinted = key(tint).convert("tinted", &model).unwrap().gltf;
        let color = |material: &Material| material.pbr_metallic_roughness.base_color_factor.0;
        assert_eq!(color(&plain)[..3], [1.0, 0.0, 0.0]);
        assert_eq!(color(&tinted)[..3], [1.0, 0.0, 1.0]);
        let extras = |material: &Material| -> Option<MaterialExtras> {
            let others = &material.extensions.as_ref()?.others;
            Some(Json::convert(others.get(MaterialExtras::EXTENSION)?).unwrap())
        };
        assert_eq!(extras(&plain), None);
        assert_eq!(extras(&tinted).map(|extras| extras.team), Some(tint));

        // An alpha-tested pass on a hue-mask texture is opaque, with no cutoff, and its extras
        // say so beside its paint.
        let hue = TeamPaint {
            tint: false,
            hue_mask: true,
        };
        let tested = MaterialKey {
            source: Source::Pass {
                shader: Shader {
                    alpha_test: true,
                    ..Shader::DEFAULT
                },
                material: VertexMaterial::DEFAULT,
            },
            ..key(hue)
        };
        let tested = tested.convert("tested", &model).unwrap().gltf;
        assert_eq!(tested.alpha_mode, Checked::Valid(AlphaMode::Opaque));
        assert!(tested.alpha_cutoff.is_none());
        assert_eq!(
            extras(&tested).map(|extras| (extras.blend, extras.team)),
            Some((Blend::Opaque, hue))
        );
    }

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
        // A hue mask's texture tests and blends by no alpha of its own: the two blends that test
        // lose their test, and the others stay.
        let masked = [
            Blend::Opaque,
            Blend::Mask { cutoff },
            Blend::Alpha,
            Blend::AlphaMask { cutoff },
            Blend::Add,
            Blend::Multiply,
            Blend::Screen,
        ]
        .map(hue_mask_blend);
        assert_eq!(
            masked,
            [
                Blend::Opaque,
                Blend::Opaque,
                Blend::Alpha,
                Blend::Alpha,
                Blend::Add,
                Blend::Multiply,
                Blend::Screen,
            ]
        );
    }
}
