use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// What a glTF material of a package says past glTF's own material, as the vendor extension
/// `MaterialExtras::EXTENSION` on it: how its color meets what is behind it, whether it writes
/// depth, its order over the primitive it covers, and how its unit's team colors it. The rest of
/// the material, its colors, its texture and how it wraps, its factors, whether it is lit and
/// two-sided, is glTF's own (02-engine-core.md, Material extras in a glTF extension).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MaterialExtras {
    pub blend: Blend,
    pub depth_write: bool,
    /// Its order over the primitive it covers, a layer of the same vertices: its depth moved
    /// toward the camera by that many steps of the depth buffer, and its sort after the ones
    /// under it, as Bevy's depth bias does; 0 for a primitive that covers none.
    pub depth_bias: i32,
    #[serde(skip_serializing_if = "TeamPaint::is_none")]
    pub team: TeamPaint,
}

/// How the team of a material's unit colors it, either or both (02-engine-core.md, Team colors
/// in the one material): `tint`, its color times the team's, as a mesh Zero Hour names
/// `HOUSECOLOR` takes the team's color; and `hue_mask`, where its texture's alpha is below one,
/// a mask and no see-through, its color of the team's hue and of its saturation times the team's,
/// as Zero Hour's textures of `ZHCA` are recolored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamPaint {
    #[serde(default)]
    pub tint: bool,
    #[serde(default)]
    pub hue_mask: bool,
}

impl TeamPaint {
    /// No team colors it.
    pub const NONE: Self = Self {
        tint: false,
        hue_mask: false,
    };

    pub const fn is_none(&self) -> bool {
        !self.tint && !self.hue_mask
    }
}

impl MaterialExtras {
    /// The name of the glTF extension that holds a material's extras.
    pub const EXTENSION: &str = "CAMPFIRE_material_extras";
}

/// A package's file is untrusted, so extras of a blend's cutoff that is not a number from 0 to 1
/// fail to read, as no renderer takes it.
impl<'de> Deserialize<'de> for MaterialExtras {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            blend: Blend,
            depth_write: bool,
            depth_bias: i32,
            #[serde(default)]
            team: TeamPaint,
        }
        let Fields {
            blend,
            depth_write,
            depth_bias,
            team,
        } = Fields::deserialize(deserializer)?;
        let cutoff = match blend {
            Blend::Mask { cutoff } | Blend::AlphaMask { cutoff } => Some(cutoff),
            Blend::Opaque | Blend::Alpha | Blend::Add | Blend::Multiply | Blend::Screen => None,
        };
        if cutoff.is_some_and(|cutoff| !(0.0..=1.0).contains(&cutoff)) {
            return Err(D::Error::custom(
                "a blend's cutoff is not a number from 0 to 1",
            ));
        }
        Ok(Self {
            blend,
            depth_write,
            depth_bias,
            team,
        })
    }
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
    use campfire_codec::Json;

    use super::*;

    #[test]
    fn extras_write_and_read_back_each_blend_and_refuse_a_cutoff_past_one() {
        for blend in [
            Blend::Opaque,
            Blend::Mask { cutoff: 0.5 },
            Blend::Alpha,
            Blend::AlphaMask { cutoff: 0.375 },
            Blend::Add,
            Blend::Multiply,
            Blend::Screen,
        ] {
            let extras = MaterialExtras {
                blend,
                depth_write: false,
                depth_bias: 2,
                team: TeamPaint::NONE,
            };
            let text = Json::write(&extras);
            assert_eq!(Json::parse::<MaterialExtras>(&text).unwrap(), extras);
            assert!(!text.contains("team"));
        }
        // A team's paint, either or both, writes and reads back; a part it does not give is off.
        let painted = MaterialExtras {
            blend: Blend::Opaque,
            depth_write: true,
            depth_bias: 0,
            team: TeamPaint {
                tint: true,
                hue_mask: true,
            },
        };
        assert_eq!(
            Json::parse::<MaterialExtras>(&Json::write(&painted)).unwrap(),
            painted
        );
        let tinted = r#"{"blend": {"mode": "opaque"}, "depth_write": true, "depth_bias": 0, "team": {"tint": true}}"#;
        let tinted = Json::parse::<MaterialExtras>(tinted).unwrap().team;
        assert_eq!((tinted.tint, tinted.hue_mask), (true, false));
        // A cutoff past 1, and a mask of none.
        let refusal = |text: &str| Json::parse::<MaterialExtras>(text).unwrap_err().to_string();
        let past =
            r#"{"blend": {"mode": "mask", "cutoff": 1.5}, "depth_write": true, "depth_bias": 0}"#;
        assert_eq!(
            refusal(past),
            "a blend's cutoff is not a number from 0 to 1"
        );
        let uncut = r#"{"blend": {"mode": "mask"}, "depth_write": true, "depth_bias": 0}"#;
        assert_eq!(refusal(uncut), "missing field `cutoff` at line 1 column 26");
    }
}
