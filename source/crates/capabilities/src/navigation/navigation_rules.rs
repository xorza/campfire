use serde::Deserialize;

use crate::units::body::Body;
use crate::units::collision_data::CollisionData;
use crate::units::layer::Layer;
use crate::values::declared_name::DeclaredName;

/// The mode's `[navigation]` section: the layers bodies move on, the first the default. A mode
/// that names none has one layer, with no name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationRules {
    #[serde(default)]
    pub layers: Vec<DeclaredName>,
}

impl NavigationRules {
    /// The layer `name`, if the mode declares it.
    pub fn layer_named(&self, name: &DeclaredName) -> Option<Layer> {
        let at = self.layers.iter().position(|layer| layer == name)?;
        Some(Layer::new(u8::try_from(at).ok()?))
    }

    /// The body of a unit type of `collision`, if it has the section: on the layer it names,
    /// which the load checked the mode declares, or on the first.
    pub fn body(&self, collision: Option<&CollisionData>) -> Option<Body> {
        let collision = collision?;
        let layer = collision.layer.as_ref().map_or(Layer::FIRST, |name| {
            self.layer_named(name).expect("the load checked the layer")
        });
        Some(collision.body.on(layer))
    }
}
