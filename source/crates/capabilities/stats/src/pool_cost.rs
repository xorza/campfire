use campfire_math::Num;
use campfire_values::NameId;

use crate::pool_id::PoolId;

/// What an action costs in each pool, 0 in a pool it does not name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolCost([Num; PoolId::LIMIT]);

impl PoolCost {
    /// The cost of `amounts`, each not negative, a pool named at most once.
    pub fn new(amounts: impl IntoIterator<Item = (PoolId, Num)>) -> Self {
        let mut cost = Self::default();
        for (pool, amount) in amounts {
            debug_assert!(amount >= Num::ZERO, "a cost is not negative");
            debug_assert_eq!(cost.0[pool.index()], Num::ZERO, "a pool is named once");
            cost.0[pool.index()] = amount;
        }
        cost
    }

    pub const fn get(&self, pool: PoolId) -> Num {
        self.0[pool.get() as usize]
    }

    /// The cost of this and `other` together; `None` past what a pool holds.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        let mut both = *self;
        for (sum, &amount) in both.0.iter_mut().zip(&other.0) {
            *sum = sum.checked_add(amount)?;
        }
        Some(both)
    }

    /// Each pool it costs something in, with the amount.
    pub fn iter(&self) -> impl Iterator<Item = (PoolId, Num)> + '_ {
        (0..).zip(self.0).filter_map(|(at, amount)| {
            let pool = PoolId::new(at).expect("a cost holds a place a pool");
            (amount > Num::ZERO).then_some((pool, amount))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_costs_add_by_pool() {
        // 2 + 3 in pool 0, 0 + 1.5 in pool 1; a sum past the largest amount is none.
        let [first, second] = [0, 1].map(|at| PoolId::new(at).unwrap());
        let one = PoolCost::new([(first, Num::int(2))]);
        let other = PoolCost::new([(first, Num::int(3)), (second, Num::ONE + Num::HALF)]);
        let both = one.checked_add(&other).unwrap();
        assert_eq!(
            both.iter().collect::<Vec<_>>(),
            [(first, Num::int(5)), (second, Num::ONE + Num::HALF)]
        );
        let full = PoolCost::new([(first, Num::MAX)]);
        assert_eq!(full.checked_add(&one), None);
    }
}
