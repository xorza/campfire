use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;

use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_id::StatId;
use crate::values::declared_name::DeclaredName;
use crate::values::names::Names;
use crate::values::stat::Stat;

/// The pools the mode declares, by pool id: the place among the stats of each one's maximum and
/// of its regen. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct PoolBook {
    /// The pools' names, by id; shared with the script view, which names them to scripts.
    names: Arc<Names<PoolId>>,
    pools: Vec<PoolStats>,
}

/// The places among the stats of a pool's maximum and of its regen a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolStats {
    pub(crate) max: StatId,
    pub(crate) regen: Option<StatId>,
}

impl PoolBook {
    /// The book of `pools`, named `names`, each pool's stats resolved by `stat`, given the pool;
    /// the first error `stat` gives.
    pub(crate) fn new<E>(
        pools: &BTreeMap<DeclaredName, PoolData>,
        names: Arc<Names<PoolId>>,
        stat: impl Fn(&DeclaredName, &Stat) -> Result<StatId, E>,
    ) -> Result<PoolBook, E> {
        assert_eq!(names.len(), pools.len(), "a pool has a name");
        let pools = pools.iter().map(|(name, pool)| {
            Ok(PoolStats {
                max: stat(name, &pool.max)?,
                regen: pool
                    .regen
                    .as_ref()
                    .map(|regen| stat(name, regen))
                    .transpose()?,
            })
        });
        Ok(PoolBook {
            names,
            pools: pools.collect::<Result<_, E>>()?,
        })
    }

    /// The pools' names, by id.
    pub(crate) const fn names(&self) -> &Arc<Names<PoolId>> {
        &self.names
    }

    /// Each pool, with its stats.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PoolId, PoolStats)> + '_ {
        (0..).zip(&self.pools).map(|(at, &stats)| {
            let pool = PoolId::new(at).expect("the load kept the pools within the limit");
            (pool, stats)
        })
    }
}
