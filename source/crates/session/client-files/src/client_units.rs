use std::collections::BTreeMap;

use campfire_values::{DeclaredName, PackagePath};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`, and each look
/// its maps' client objects name, `[objects.<look>]`, which a game may draw otherwise than its
/// units (design 12, D14).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnits {
    pub units: BTreeMap<DeclaredName, ClientUnit>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<DeclaredName, ClientUnit>,
}

/// A unit type's look: the models drawn at its unit's place, all of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnit {
    pub models: Vec<ClientModel>,
}

/// A model, a `.glb` of the package, the scale it is drawn at, and the names of its nodes its
/// default look hides, as the glTF names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientModel {
    pub model: PackagePath,
    #[serde(default, skip_serializing_if = "ModelScale::is_one")]
    pub scale: ModelScale,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
}

/// The factor a model is drawn at, positive and finite, 1 when a file gives none.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ModelScale(f32);

impl ModelScale {
    pub const ONE: ModelScale = ModelScale(1.0);

    /// `None` unless `factor` is positive and finite.
    pub const fn new(factor: f32) -> Option<ModelScale> {
        if factor.is_finite() && factor > 0.0 {
            Some(ModelScale(factor))
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
    fn default() -> ModelScale {
        ModelScale::ONE
    }
}

/// A package's file is untrusted, so a scale that is not positive and finite fails to read.
impl<'de> Deserialize<'de> for ModelScale {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ModelScale, D::Error> {
        let factor = f32::deserialize(deserializer)?;
        ModelScale::new(factor)
            .ok_or_else(|| D::Error::custom("a model's scale is positive and finite"))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn units_write_and_read_back_their_models_and_hidden_nodes() {
        let text = r#"
            [units.humvee]
            models = [{ model = "client/models/avhummer.glb", hide = ["turretup01", "muzzlefx01"] }]

            [units.tree]
            models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]

            [objects.tree]
            models = [{ model = "client/models/ptxbirch04.glb", scale = 1.25 }]
        "#;
        let units: ClientUnits = Toml::parse(text).unwrap();
        let humvee = &units.units["humvee"].models[0];
        assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
        assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
        assert!(units.units["tree"].models[0].hide.is_empty());
        // A model's scale, 1 when it gives none, which it does not write; a client object's look.
        assert_eq!(
            (humvee.scale, units.units["tree"].models[0].scale),
            (ModelScale::ONE, ModelScale::new(1.25).unwrap())
        );
        assert!(!Toml::write(&units).contains("scale = 1.0"));
        assert_eq!(units.objects["tree"], units.units["tree"]);
        assert_eq!(
            Toml::parse::<ClientUnits>(&Toml::write(&units)).unwrap(),
            units
        );
        // A scale of 0, below 0 or not finite; a path that leaves the package, a name that is no
        // declared name, an unknown field.
        for scale in ["0.0", "-0.0", "-1.0", "nan", "inf"] {
            let text = format!("[units.a]\nmodels = [{{ model = \"a.glb\", scale = {scale} }}]");
            assert!(Toml::parse::<ClientUnits>(&text).is_err(), "{scale}");
        }
        assert!(
            Toml::parse::<ClientUnits>(
                r#"[units.a]
models = [{ model = "../x.glb" }]"#
            )
            .is_err()
        );
        assert!(
            Toml::parse::<ClientUnits>(
                "[units.A]
models = []"
            )
            .is_err()
        );
        assert!(
            Toml::parse::<ClientUnits>(
                r#"[units.a]
models = []
model = "x""#
            )
            .is_err()
        );
    }
}
