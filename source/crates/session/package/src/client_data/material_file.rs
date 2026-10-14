use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

/// A material file, `client/materials/<name>.toml`, which overrides the glTF material of that name
/// whole, for what glTF's material cannot say, so it holds all a glTF material of the import
/// holds. Colors are linear, as glTF's factors are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialFile {
    /// Red, green, blue and alpha, each 0 to 1, multiplying the texture's.
    pub base_color: [f32; 4],
    /// The texture of the base color, a KTX2 file of the package.
    pub texture: Option<MaterialTexture>,
    /// Red, green and blue, each 0 to 1, added as light.
    pub emissive: [f32; 3],
    /// How metallic and how rough it is, each 0 to 1, as glTF's factors.
    pub metallic: f32,
    pub roughness: f32,
    /// The strength of its specular reflection, 0 to 1, as `KHR_materials_specular`'s factor: 1
    /// a dielectric's usual reflectance.
    pub specular: f32,
    /// Whether it is drawn of its colors and texture alone, of no light, as `KHR_materials_unlit`.
    pub unlit: bool,
    pub blend: Blend,
    pub depth_write: bool,
    /// Its order over the primitive it covers, a layer of the same vertices: its depth moved
    /// toward the camera by that many steps of the depth buffer, and its sort after the ones
    /// under it, as Bevy's depth bias does; 0 for a primitive that covers none.
    pub depth_bias: i32,
    pub double_sided: bool,
}

/// A material's texture and how it wraps on each axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialTexture {
    pub path: PackagePath,
    pub clamp_u: bool,
    pub clamp_v: bool,
}

/// How a material's color meets what is drawn behind it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Blend {
    /// It replaces what is behind.
    Opaque,
    /// It replaces what is behind where its alpha reaches `cutoff`, and is not drawn elsewhere.
    Mask { cutoff: f32 },
    /// It mixes with what is behind by its alpha.
    Alpha,
    /// It mixes by its alpha where its alpha reaches `cutoff`, and is not drawn elsewhere.
    AlphaMask { cutoff: f32 },
    /// It adds its color to what is behind.
    Add,
    /// It multiplies what is behind by its color.
    Multiply,
    /// It adds its color to what is behind, scaled by one less what is behind.
    Screen,
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_material_writes_and_reads_back_each_blend() {
        let text = r#"
            base_color = [1.0, 0.5, 0.25, 1.0]
            emissive = [0.0, 0.0, 0.0]
            metallic = 0.0
            roughness = 1.0
            specular = 0.0
            unlit = true
            blend = { mode = "alpha_mask", cutoff = 0.375 }
            depth_write = true
            depth_bias = 2
            double_sided = false
            texture = { path = "client/textures/art/textures/fx.ktx2", clamp_u = true, clamp_v = false }
        "#;
        let material: MaterialFile = Toml::parse(text).unwrap();
        assert_eq!(material.blend, Blend::AlphaMask { cutoff: 0.375 });
        assert_eq!(
            (
                material.metallic,
                material.roughness,
                material.specular,
                material.unlit
            ),
            (0.0, 1.0, 0.0, true)
        );
        assert_eq!(material.depth_bias, 2);
        assert_eq!(
            material.texture.as_ref().unwrap().path.as_str(),
            "client/textures/art/textures/fx.ktx2"
        );
        for blend in [
            Blend::Opaque,
            Blend::Mask { cutoff: 0.5 },
            Blend::Alpha,
            Blend::AlphaMask { cutoff: 0.5 },
            Blend::Add,
            Blend::Multiply,
            Blend::Screen,
        ] {
            let with = MaterialFile {
                blend,
                texture: None,
                ..material.clone()
            };
            assert_eq!(
                Toml::parse::<MaterialFile>(&Toml::write(&with)).unwrap(),
                with
            );
        }
        assert!(Toml::parse::<MaterialFile>(&text.replace("alpha_mask", "glow")).is_err());
        // Each of a glTF material's fields is the file's to give.
        assert!(Toml::parse::<MaterialFile>(&text.replace("unlit = true", "")).is_err());
    }
}
