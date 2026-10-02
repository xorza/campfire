use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;

use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;
use crate::values::declared_name::DeclaredName;

/// The pools the mode declares, by pool id: the place among the stats of each one's maximum and
/// of its regen. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct PoolBook {
    /// Shared with the script view, which names them to scripts.
    names: Arc<[DeclaredName]>,
    pools: Vec<PoolStats>,
}

/// The places among the stats of a pool's maximum and of its regen a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolStats {
    pub(crate) max: StatId,
    pub(crate) regen: Option<StatId>,
}

impl PoolBook {
    /// The book of `pools`, whose stats the load checked `stats` declares.
    pub(crate) fn new(pools: &BTreeMap<DeclaredName, PoolData>, stats: &StatBook) -> PoolBook {
        let index = |stat| {
            stats
                .index(stat)
                .expect("the load checked the pools' stats")
        };
        PoolBook {
            names: pools.keys().cloned().collect(),
            pools: pools
                .values()
                .map(|pool| PoolStats {
                    max: index(&pool.max),
                    regen: pool.regen.as_ref().map(index),
                })
                .collect(),
        }
    }

    /// The pools, by id.
    pub(crate) fn names(&self) -> Arc<[DeclaredName]> {
        Arc::clone(&self.names)
    }

    /// Each pool, with its stats.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PoolId, PoolStats)> + '_ {
        (0..).zip(&self.pools).map(|(at, &stats)| {
            let pool = PoolId::new(at).expect("the load kept the pools within the limit");
            (pool, stats)
        })
    }
}
