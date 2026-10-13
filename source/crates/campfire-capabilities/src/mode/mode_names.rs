use std::sync::Arc;

use crate::mode::error::{ModeList, ModeNamesError};
use crate::mode::mode_data::ModeData;
use crate::players::resource_id::ResourceId;
use crate::scripts::error::UnknownName;
use crate::scripts::name_kind::NameKind;
use crate::stats::pool_id::PoolId;
use crate::units::body_form::BodyForm;
use crate::units::collision_data::CollisionData;
use crate::units::layer::Layer;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::error::NamesError;
use crate::values::names::{NameId, Names};

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
    /// The names of `data`; an error for a list with a name twice, or more names than the ids of
    /// their kind count.
    pub(crate) fn of(data: &ModeData) -> Result<ModeNames, ModeNamesError> {
        Ok(ModeNames {
            pools: Arc::new(ModeNames::list(data.pools.keys(), ModeList::Pools)?),
            resources: Arc::new(ModeNames::list(&data.resources, ModeList::Resources)?),
            damage_kinds: Arc::new(ModeNames::list(
                &data.combat.damage_kinds,
                ModeList::DamageKinds,
            )?),
            layers: ModeNames::list(&data.navigation.layers, ModeList::Layers)?,
        })
    }

    /// The table of `names`, the mode's list `list`, each the next id.
    fn list<'n, I: NameId<Scope = ()>>(
        names: impl IntoIterator<Item = &'n DeclaredName>,
        list: ModeList,
    ) -> Result<Names<I>, ModeNamesError> {
        let mut table = Names::default();
        for name in names {
            table.try_add(name.as_str()).map_err(|error| match error {
                NamesError::Full => ModeNamesError::TooMany(list),
                NamesError::Repeated => ModeNamesError::Repeated {
                    list,
                    name: name.clone(),
                },
            })?;
        }
        Ok(table)
    }

    /// The body of a unit type of `collision`, if it has the section: on the layer it names, or
    /// on the first; an error for a layer the mode does not declare.
    pub(crate) fn form(
        &self,
        collision: Option<&CollisionData>,
    ) -> Result<Option<BodyForm>, UnknownName> {
        let Some(collision) = collision else {
            return Ok(None);
        };
        let layer = match &collision.layer {
            Some(name) => self
                .layers
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Layer, name))?,
            None => Layer::FIRST,
        };
        Ok(Some(collision.form.on(layer)))
    }
}
