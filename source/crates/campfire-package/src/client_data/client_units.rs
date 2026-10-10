use std::collections::BTreeMap;

use campfire_capabilities::{DeclaredName, PackagePath};
use serde::{Deserialize, Serialize};

/// A package's `client/units.toml`: how each unit type looks, `[units.<type>]`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnits {
    pub units: BTreeMap<DeclaredName, ClientUnit>,
}

/// A unit type's look: the models drawn at its unit's place, all of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientUnit {
    pub models: Vec<ClientModel>,
}

/// A model, a `.glb` of the package, and the names of its nodes its default look hides, as the
/// glTF names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientModel {
    pub model: PackagePath,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
}

#[cfg(test)]
mod tests {
    use campfire_common::Toml;

    use super::*;

    #[test]
    fn units_write_and_read_back_their_models_and_hidden_nodes() {
        let text = r#"
            [units.humvee]
            models = [{ model = "client/models/avhummer.glb", hide = ["turretup01", "muzzlefx01"] }]

            [units.tree]
            models = [{ model = "client/models/ptxbirch04.glb" }]
        "#;
        let units: ClientUnits = Toml::parse(text).unwrap();
        let humvee = &units.units["humvee"].models[0];
        assert_eq!(humvee.model.as_str(), "client/models/avhummer.glb");
        assert_eq!(humvee.hide, ["turretup01", "muzzlefx01"]);
        assert!(units.units["tree"].models[0].hide.is_empty());
        assert_eq!(
            Toml::parse::<ClientUnits>(&Toml::write(&units).unwrap()).unwrap(),
            units
        );
        // A path that leaves the package, a name that is no declared name, an unknown field.
        assert!(
            Toml::parse::<ClientUnits>(
                r#"[units.a]
models = [{ model = "../x.glb" }]"#
            )
            .is_err()
        );
        assert!(
            Toml::parse::<ClientUnits>(
                r"[units.A]
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
