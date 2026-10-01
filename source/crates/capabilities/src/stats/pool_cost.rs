use campfire_math::Num;

use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;

/// What an action costs in each pool, 0 in a pool it does not name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolCost([Num; Pools::LIMIT]);

impl PoolCost {
    /// The cost of `amounts`, each not negative, a pool named at most once.
    pub fn new(amounts: impl IntoIterator<Item = (PoolId, Num)>) -> PoolCost {
        let mut cost = PoolCost::default();
        for (pool, amount) in amounts {
            debug_assert!(amount >= Num::ZERO, "a cost is not negative");
            debug_assert_eq!(cost.0[pool.index()], Num::ZERO, "a pool is named once");
            cost.0[pool.index()] = amount;
        }
        cost
    }

    pub const fn get(&self, pool: PoolId) -> Num {
        self.0[pool.index()]
    }

    /// Each pool it costs something in, with the amount.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PoolId, Num)> + '_ {
        (0..).zip(self.0).filter_map(|(at, amount)| {
            let pool = PoolId::new(at).expect("a cost holds a place a pool");
            (amount > Num::ZERO).then_some((pool, amount))
        })
    }
}
