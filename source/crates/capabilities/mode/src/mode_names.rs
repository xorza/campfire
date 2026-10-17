use std::sync::Arc;

use campfire_combat::ModeCombatData;
use campfire_common::DeclaredName;
use campfire_navigation::ModeNavigationData;
use campfire_players::ResourcesData;
use campfire_script::{NameKind, UnknownName};
use campfire_stats::{ModePoolsData, PoolId};
use campfire_units::{BodyForm, CollisionData, Layer};
use campfire_values::{DamageKind, NameId, Names, NamesError, ResourceId};

use crate::error::{ModeList, ModeNamesError};
use crate::mode_data::ModeData;

/// The names the mode's data declares for its pools, its players' resources, its damage kinds
/// and its layers, each kind in its table: what the books resolve the packages' names by, and
/// share.
#[derive(Debug, Clone, Default)]
pub struct ModeNames {
    pub pools: Arc<Names<PoolId>>,
    pub resources: Arc<Names<ResourceId>>,
    pub damage_kinds: Arc<Names<DamageKind>>,
    pub layers: Names<Layer>,
}

impl ModeNames {
    /// The names of `data`; an error for a list with a name twice, or more names than the ids of
    /// their kind count.
    pub fn of(data: &ModeData) -> Result<Self, ModeNamesError> {
        let pools = data.section_or_default::<ModePoolsData>();
        let resources = data.section_or_default::<ResourcesData>();
        let combat = data.section_or_default::<ModeCombatData>();
        let navigation = data.section_or_default::<ModeNavigationData>();
        Ok(Self {
            pools: Arc::new(Self::list(pools.0.keys(), ModeList::Pools)?),
            resources: Arc::new(Self::list(&resources.0, ModeList::Resources)?),
            damage_kinds: Arc::new(Self::list(&combat.damage_kinds, ModeList::DamageKinds)?),
            layers: Self::list(&navigation.layers, ModeList::Layers)?,
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
    pub fn form(&self, collision: Option<&CollisionData>) -> Result<Option<BodyForm>, UnknownName> {
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
