use std::sync::Arc;

use crate::mode::mode_data::ModeData;
use crate::players::resource_id::ResourceId;
use crate::stats::pool_id::PoolId;
use crate::units::body_form::BodyForm;
use crate::units::collision_data::CollisionData;
use crate::units::layer::Layer;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::names::Names;

/// The names the mode's data declares for its pools, its players' resources, its damage kinds
/// and its layers, each kind in its table: what the books resolve the packages' names by, and
/// share.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModeNames {
    pub(crate) pools: Arc<Names<PoolId>>,
    pub(crate) resources: Arc<Names<ResourceId>>,
    pub(crate) damage_kinds: Arc<Names<DamageKind>>,
    pub(crate) layers: Names<Layer>,
}

impl ModeNames {
    /// The names of `data`, which the package load checked: none twice, and each kind within
    /// its limit.
    pub(crate) fn of(data: &ModeData) -> ModeNames {
        ModeNames {
            pools: Arc::new(data.pools.keys().map(DeclaredName::as_str).collect()),
            resources: Arc::new(data.resources.iter().map(DeclaredName::as_str).collect()),
            damage_kinds: Arc::new(
                data.combat
                    .damage_kinds
                    .iter()
                    .map(DeclaredName::as_str)
                    .collect(),
            ),
            layers: data
                .navigation
                .layers
                .iter()
                .map(DeclaredName::as_str)
                .collect(),
        }
    }

    /// The body of a unit type of `collision`, if it has the section: on the layer it names,
    /// which the load checked the mode declares, or on the first.
    pub(crate) fn form(&self, collision: Option<&CollisionData>) -> Option<BodyForm> {
        let collision = collision?;
        let layer = collision.layer.as_ref().map_or(Layer::FIRST, |name| {
            let layer = self.layers.named(name.as_str());
            layer.expect("the load checked the layer")
        });
        Some(collision.form.on(layer))
    }
}
