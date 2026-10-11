use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{SimComponent, TickRate};
use serde::{Deserialize, Serialize};

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnChange;
use crate::stats::meter::Meter;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pool_id::PoolId;

/// A unit's pools, by pool id: each an amount from 0 to its maximum, none where its type lists
/// no such pool. A fixed array, so copying a unit's pools, as a rollback does every frame,
/// allocates nothing.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pools([Option<Meter>; PoolId::LIMIT]);

impl Pools {
    /// Full pools of each maximum in `maxes`, a pool named at most once; `None` unless every
    /// maximum is positive.
    pub fn new(maxes: impl IntoIterator<Item = (PoolId, Num)>) -> Option<Pools> {
        let mut pools = Pools([None; PoolId::LIMIT]);
        for (pool, max) in maxes {
            debug_assert!(pools.0[pool.index()].is_none(), "a pool is named once");
            pools.0[pool.index()] = Some(Meter::new(max)?);
        }
        Some(pools)
    }

    /// The pools it has.
    pub fn ids(&self) -> impl Iterator<Item = PoolId> + '_ {
        (0..)
            .zip(&self.0)
            .filter(|(_, meter)| meter.is_some())
            .map(|(at, _)| PoolId::new(at).expect("a place among the pools"))
    }

    /// The current amount of `pool`; `None` when it has no such pool.
    pub fn current(&self, pool: PoolId) -> Option<Num> {
        self.0[pool.index()].map(Meter::current)
    }

    /// The maximum of `pool`; `None` when it has no such pool.
    pub fn max(&self, pool: PoolId) -> Option<Num> {
        self.0[pool.index()].map(Meter::max)
    }

    /// Whether it has `pool`, and the pool is above 0.
    pub fn above_zero(&self, pool: PoolId) -> bool {
        self.0[pool.index()].is_some_and(|meter| !meter.is_empty())
    }

    /// Whether it has every pool `cost` names, each holding at least its amount.
    pub(crate) fn affords(&self, cost: &PoolCost) -> bool {
        cost.iter()
            .all(|(pool, amount)| self.current(pool).is_some_and(|current| current >= amount))
    }

    /// Spends `cost`, which it affords.
    pub(crate) fn pay(&mut self, cost: &PoolCost) {
        debug_assert!(self.affords(cost), "an action pays what it affords");
        for (pool, amount) in cost.iter() {
            self.meter(pool).take(amount);
        }
    }

    /// Takes `amount`, which is not negative, from `pool`, down to 0; what it took.
    pub(crate) fn take(&mut self, pool: PoolId, amount: Num) -> Num {
        let meter = self.meter(pool);
        let before = meter.current();
        meter.take(amount);
        before - meter.current()
    }

    /// Adds `amount`, which is not negative, to `pool`, up to its maximum, when it has the pool.
    pub(crate) fn add(&mut self, pool: PoolId, amount: Num) {
        if let Some(meter) = &mut self.0[pool.index()] {
            meter.add(amount);
        }
    }

    /// Sets the maximum of `pool`, as `Meter::set_max` does, when it has the pool.
    pub(crate) fn set_max(&mut self, pool: PoolId, max: Num) {
        if let Some(meter) = &mut self.0[pool.index()] {
            meter.set_max(max);
        }
    }

    /// Adds a tick's share of `per_second` to `pool`, as `Meter::regen` does, when it has the
    /// pool.
    pub(crate) fn regen(&mut self, pool: PoolId, per_second: Num, hz: u32) {
        if let Some(meter) = &mut self.0[pool.index()] {
            meter.regen(per_second, hz);
        }
    }

    /// Fills every pool it has.
    pub(crate) fn fill(&mut self) {
        for meter in self.0.iter_mut().flatten() {
            meter.fill();
        }
    }

    /// The meter of `pool`, which it has.
    const fn meter(&mut self, pool: PoolId) -> &mut Meter {
        self.0[pool.index()]
            .as_mut()
            .expect("a unit has the pool it pays from or takes from")
    }
}

impl SimComponent for Pools {
    const NAME: &'static str = "stats.pools";

    // A fixed array of every pool place, whose meters' decode keeps each amount within its maximum;
    // a place the mode does not declare is never read. What a meter carries, regen adds: more
    // than the match's rate leaves would regenerate past a pool's rate.
    fn check(&self, world: &World, _: Entity) -> bool {
        let hz = world.resource::<TickRate>().hz().get();
        self.0.iter().flatten().all(|meter| meter.settled(hz))
    }
}

impl Replication for Pools {
    const KIND: DataKind = DataKind::Life;
    type Sending = SentOnChange;
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::stats::pool_id::PoolId;
    use crate::stats::pools::Pools;

    /// `pools` with `amount` of `pool`, which it has, spent.
    pub fn spent(mut pools: Pools, pool: PoolId, amount: Num) -> Pools {
        pools.take(pool, amount);
        pools
    }

    #[cfg(test)]
    impl Pools {
        /// A full life pool of `max` and no other pool, the life pool the first, as it is until a
        /// mode binds one.
        pub(crate) fn life(max: Num) -> Pools {
            Pools::new([(PoolId::FIRST, max)]).expect("a positive maximum")
        }
    }
}
