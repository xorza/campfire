use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`, and each look
/// its maps' client objects name, `[objects.<look>]`, which a game may draw otherwise than its
/// units (09-zero-hour.md, The multiplayer rule for client objects).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnits {
    pub units: BTreeMap<DeclaredName, ClientUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<DeclaredName, ClientUnit>,
}

/// A unit type's look: the models drawn at its unit's place, all of them; its variants, the
/// models it draws in their stead on a map of their conditions; and the angle in degrees a
/// client's preview of a build places it at, 0 when it gives none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnit {
    pub models: Vec<ClientModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<LookVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_angle: Option<Num>,
    /// The name of the node its models turn by the unit's turret angle
    /// (04-capabilities/combat.md, A unit type may have a turret).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
}

/// A look's models on a map of some conditions, as a night or snowy weather, by name: any game
/// names its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookVariant {
    pub conditions: Vec<DeclaredName>,
    pub models: Vec<ClientModel>,
}

impl ClientUnits {
    /// Where a package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/units.toml")
    }
}

impl ClientUnit {
    /// The look of `models`, of no variant.
    pub const fn new(models: Vec<ClientModel>) -> Self {
        Self {
            models,
            variants: Vec::new(),
            placement_angle: None,
            turret: None,
        }
    }

    /// The top of the highest of its models, of those whose top is known; none when none is.
    pub fn top(&self) -> Option<ModelTop> {
        let tops = self.models.iter().filter_map(|model| model.top);
        tops.reduce(|top, other| if other.0 > top.0 { other } else { top })
    }

    /// Draws, on a map of `conditions`, the models of its variant whose conditions have the most
    /// in common with the map's, of a tie the one of the fewest the map lacks, of a tie the first,
    /// its own models counted first as of no condition; and holds no variant after.
    pub fn settle(&mut self, conditions: &[DeclaredName]) {
        let mut best = None;
        let (mut most, mut fewest) = (0, 0);
        for (at, variant) in self.variants.iter().enumerate() {
            let common = variant
                .conditions
                .iter()
                .filter(|condition| conditions.contains(condition))
                .count();
            let lacked = variant.conditions.len() - common;
            if common > most || (common >= most && lacked < fewest) {
                (best, most, fewest) = (Some(at), common, lacked);
            }
        }
        let mut variants = std::mem::take(&mut self.variants);
        if let Some(at) = best {
            self.models = variants.swap_remove(at).models;
        }
    }
}

/// A model, a `.glb` of the package, the scale it is drawn at, the names of its nodes its
/// default look hides, as the glTF names them, and the top of what it draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientModel {
    pub model: PackagePath,
    #[serde(default, skip_serializing_if = "ModelScale::is_one")]
    pub scale: ModelScale,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
    /// None for a model that draws nothing, or whose file does not say
    /// (02-engine-core.md, The import measures a look's height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<ModelTop>,
}

/// The height in meters of the top of what a model draws over its origin, at its scale, with
/// its hidden nodes hidden: finite, and below the origin for a model drawn under it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelTop(f32);

impl ModelTop {
    /// `None` unless `height` is finite.
    pub const fn new(height: f32) -> Option<Self> {
        if height.is_finite() {
            Some(Self(height))
        } else {
            None
        }
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A top is never NaN.
impl Eq for ModelTop {}

/// A package's file is untrusted, so a top that is not finite fails to read.
impl<'de> Deserialize<'de> for ModelTop {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let height = f32::deserialize(deserializer)?;
        Self::new(height).ok_or_else(|| D::Error::custom("a model's top is finite"))
    }
}

/// The factor a model is drawn at, positive and finite, 1 when a file gives none.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ModelScale(f32);

impl ModelScale {
    pub const ONE: Self = Self(1.0);

    /// `None` unless `factor` is positive and finite.
    pub const fn new(factor: f32) -> Option<Self> {
        if factor.is_finite() && factor > 0.0 {
            Some(Self(factor))
        } else {
            None
        }
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde passes a field by reference"
    )]
    const fn is_one(&self) -> bool {
        self.0 == 1.0
    }
}

/// A scale is never NaN.
impl Eq for ModelScale {}

impl Default for ModelScale {
    fn default() -> Self {
        Self::ONE
    }
}

/// A package's file is untrusted, so a scale that is not positive and finite fails to read.
impl<'de> Deserialize<'de> for ModelScale {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let factor = f32::deserialize(deserializer)?;
        Self::new(factor).ok_or_else(|| D::Error::custom("a model's scale is positive and finite"))
    }
}

impl ClientFile for ClientUnits {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn units_write_and_read_back_their_models_and_hidden_nodes() {
        let text = r#"
            [units.humvee]
            models = [
                { model = "client/models/avhummer.glb", hide = ["turretup01", "muzzlefx01"], top = 2.5 },
                { model = "client/models/avhumflag.glb", top = 3.25 },
                { model = "client/models/avhumfx.glb" },
            ]

            [units.tree]
            models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]

            [units.barracks]
            models = []
            placement_angle = "-135"

            [objects.tree]
            models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]
        "#;
        let units: ClientUnits = Toml::parse(text).unwrap();
        let humvee = &units.units["humvee"].models[0];
        assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
        assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
        assert!(units.units["tree"].models[0].hide.is_empty());
        // The look's top, its flag's 3.25 m over the hull's 2.5 m, of a model whose top it does
        // not know, which it does not write; none of a look of no known top.
        let top = |look: &str| units.units[look].top().map(ModelTop::get);
        assert_eq!((top("humvee"), top("tree")), (Some(3.25), None));
        assert_eq!(Toml::write(&units).matches("top = ").count(), 2);
        // A model's scale, 1 when it gives none, which it does not write; a client object's look.
        assert_eq!(
            (humvee.scale, units.units["tree"].models[0].scale),
            (ModelScale::ONE, ModelScale::new(1.25).unwrap())
        );
        assert!(!Toml::write(&units).contains("scale = 1.0"));
        assert_eq!(units.objects["tree"], units.units["tree"]);
        // A build's angle; none for a look that gives none, which it does not write.
        assert_eq!(
            units.units["barracks"].placement_angle,
            Some(Num::int(-135))
        );
        assert_eq!(units.units["tree"].placement_angle, None);
        assert_eq!(Toml::write(&units).matches("placement_angle").count(), 1);
        assert_eq!(
            Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
            units
        );
        // A scale of 0, below 0 or not finite; a path that leaves the package, a name that is no
        // declared name, an unknown field.
        let refusal = |text: &str| {
            let refused = Toml::parse::<ClientUnits>(text).unwrap_err();
            refused.message().to_owned()
        };
        for scale in ["0.0", "-0.0", "-1.0", "nan", "inf"] {
            let text = format!("[units.a]\nmodels = [{{ model = \"a.glb\", scale = {scale} }}]");
            assert_eq!(
                refusal(&text),
                "a model's scale is positive and finite",
                "{scale}"
            );
        }
        // A top not finite; a top below the origin reads.
        let top = |top: &str| format!("[units.a]\nmodels = [{{ model = \"a.glb\", top = {top} }}]");
        for wrong in ["nan", "-inf"] {
            assert_eq!(refusal(&top(wrong)), "a model's top is finite", "{wrong}");
        }
        Toml::parse::<ClientUnits>(&top("-0.5")).unwrap();
        assert_eq!(
            refusal("[units.a]\nmodels = [{ model = \"../x.glb\" }]"),
            "\"../x.glb\": no path in the package"
        );
        let name = refusal("[units.A]\nmodels = []");
        assert!(name.starts_with("\"A\" is not a name"), "{name}");
        let field = refusal("[units.a]\nmodels = []\nmodel = \"x\"");
        assert!(field.starts_with("unknown field `model`"), "{field}");
    }

    #[test]
    fn a_look_draws_its_variant_that_best_matches_the_maps_conditions() {
        let text = r#"
            [units.barracks]
            models = [{ model = "day.glb" }]
            variants = [
                { conditions = ["night"], models = [{ model = "night.glb" }] },
                { conditions = ["snow"], models = [{ model = "snow.glb" }] },
                { conditions = ["night", "snow"], models = [{ model = "both.glb" }] },
                { conditions = ["night", "damaged"], models = [{ model = "hurt.glb" }] },
            ]
        "#;
        let units: ClientUnits = Toml::parse(text).unwrap();
        assert_eq!(
            Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
            units
        );
        let drawn = |conditions: &[&str]| {
            let conditions: Vec<DeclaredName> = conditions
                .iter()
                .map(|condition| DeclaredName::new(condition).unwrap())
                .collect();
            let mut look = units.units["barracks"].clone();
            look.settle(&conditions);
            assert!(look.variants.is_empty());
            look.models[0].model.as_str().to_owned()
        };
        // A day map has no condition in common with a variant: the look's own. A night map: the
        // night variant, 1 in common and none lacked, before both, of 1 and 1 lacked, and the
        // damaged night, of 1 and 1. Night and snow: both, of 2. Rain, of none: the look's own.
        assert_eq!(drawn(&[]), "day.glb");
        assert_eq!(drawn(&["night"]), "night.glb");
        assert_eq!(drawn(&["snow"]), "snow.glb");
        assert_eq!(drawn(&["night", "snow"]), "both.glb");
        assert_eq!(drawn(&["rain"]), "day.glb");
        // Night and damage: the damaged night, of 2 in common, past the night's 1.
        assert_eq!(drawn(&["night", "damaged"]), "hurt.glb");
    }
}
