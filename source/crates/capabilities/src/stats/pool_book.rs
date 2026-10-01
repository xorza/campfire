use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;

use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::values::declared_name::DeclaredName;

/// The pools the mode declares, by pool id: the place among the stats of each one's maximum and
/// of its regen. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct PoolBook {
    pools: Vec<PoolStats>,
}

/// The places among the stats of a pool's maximum and of its regen a second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolStats {
    pub(crate) max: u16,
    pub(crate) regen: Option<u16>,
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
            pools: pools
                .values()
                .map(|pool| PoolStats {
                    max: index(&pool.max),
                    regen: pool.regen.as_ref().map(index),
                })
                .collect(),
        }
    }

    /// Each pool, with its stats.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PoolId, PoolStats)> + '_ {
        (0..).zip(&self.pools).map(|(at, &stats)| {
            let pool = PoolId::new(at).expect("the load kept the pools within the limit");
            (pool, stats)
        })
    }
}
