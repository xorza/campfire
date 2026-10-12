use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::world::World;
use campfire_math::Num;

use crate::stats::life::Life;
use crate::stats::meter::Meter;
use crate::stats::pool_book::PoolBook;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;

/// A unit's pools by pool id, its life among them: `Life` and `Pools` as one, for a rule that
/// names a pool by its id, as a cost, a regen or a restore does. The life pool's id, which no
/// component holds, comes with each view.
#[derive(QueryData, Debug)]
#[query_data(mutable, derive(Debug))]
pub(crate) struct UnitPools {
    life: Option<&'static mut Life>,
    pools: Option<&'static mut Pools>,
}

/// A unit's pools to read by pool id: `life` the meter of the mode's life pool, `life_pool`, and
/// `pools` its other pools.
#[derive(Debug, Clone, Copy)]
pub struct PoolMeters<'a> {
    life_pool: Option<PoolId>,
    life: Option<&'a Life>,
    pools: Option<&'a Pools>,
}

/// A unit's pools to change by pool id, as `PoolMeters` reads them. Each write marks only the
/// component whose meter it changes, and only when it changes it.
#[derive(Debug)]
pub(crate) struct PoolMetersMut<'a> {
    life_pool: Option<PoolId>,
    life: Option<Mut<'a, Life>>,
    pools: Option<Mut<'a, Pools>>,
}

impl<'w> UnitPoolsItem<'w, '_> {
    /// Its pools, the life pool `life_pool`.
    pub(crate) fn of(self, life_pool: Option<PoolId>) -> PoolMetersMut<'w> {
        PoolMetersMut {
            life_pool,
            life: self.life,
            pools: self.pools,
        }
    }

    /// Fills its life and every other pool it has.
    pub(crate) fn fill(&mut self) {
        if let Some(life) = &mut self.life {
            life.fill();
        }
        if let Some(pools) = &mut self.pools {
            pools.fill();
        }
    }
}

impl<'w> UnitPoolsReadOnlyItem<'w, '_> {
    /// Its pools, the life pool `life_pool`.
    pub(crate) const fn of(self, life_pool: Option<PoolId>) -> PoolMeters<'w> {
        PoolMeters::new(life_pool, self.life, self.pools)
    }
}

impl<'a> PoolMeters<'a> {
    pub(crate) const fn new(
        life_pool: Option<PoolId>,
        life: Option<&'a Life>,
        pools: Option<&'a Pools>,
    ) -> PoolMeters<'a> {
        PoolMeters {
            life_pool,
            life,
            pools,
        }
    }

    /// The meter of `pool`; `None` when the unit has no such pool.
    pub(crate) fn meter(&self, pool: PoolId) -> Option<Meter> {
        if Some(pool) == self.life_pool {
            self.life.map(|life| life.meter())
        } else {
            self.pools?.meter(pool)
        }
    }

    /// The current amount of `pool`; `None` when the unit has no such pool.
    pub fn current(&self, pool: PoolId) -> Option<Num> {
        self.meter(pool).map(Meter::current)
    }

    /// Whether it has every pool `cost` names, each holding at least its amount.
    pub(crate) fn affords(&self, cost: &PoolCost) -> bool {
        cost.iter()
            .all(|(pool, amount)| self.current(pool).is_some_and(|current| current >= amount))
    }

    /// The pools of the unit of `entity`, the life pool the match's.
    pub fn of(world: &World, entity: Entity) -> PoolMeters<'_> {
        let life_pool = world.get_resource::<PoolBook>().and_then(PoolBook::life);
        PoolMeters::new(life_pool, world.get(entity), world.get(entity))
    }
}

impl PoolMetersMut<'_> {
    pub(crate) fn read(&self) -> PoolMeters<'_> {
        PoolMeters::new(self.life_pool, self.life.as_deref(), self.pools.as_deref())
    }

    /// Changes the meter of `pool` by `change`, when the unit has the pool.
    fn change(&mut self, pool: PoolId, change: impl FnOnce(&mut Meter)) {
        let meter = if Some(pool) == self.life_pool {
            let life = self.life.as_mut().map(Mut::reborrow);
            life.map(|life| life.map_unchanged(Life::meter_mut))
        } else {
            let pools = self.pools.as_mut().map(Mut::reborrow);
            pools.and_then(|pools| pools.filter_map_unchanged(|pools| pools.meter_mut(pool)))
        };
        if let Some(mut meter) = meter {
            let mut changed = *meter;
            change(&mut changed);
            meter.set_if_neq(changed);
        }
    }

    /// Spends `cost`, which it affords.
    pub(crate) fn pay(&mut self, cost: &PoolCost) {
        debug_assert!(self.read().affords(cost), "a unit pays what it affords");
        for (pool, amount) in cost.iter() {
            self.change(pool, |meter| meter.take(amount));
        }
    }

    /// Adds `amount`, which is not negative, to `pool`, up to its maximum, when it has the pool.
    pub(crate) fn add(&mut self, pool: PoolId, amount: Num) {
        self.change(pool, |meter| meter.add(amount));
    }

    /// Sets the maximum of `pool`, as `Meter::set_max` does, when it has the pool.
    pub(crate) fn set_max(&mut self, pool: PoolId, max: Num) {
        self.change(pool, |meter| meter.set_max(max));
    }

    /// Adds a tick's share of `per_second` to `pool`, as `Meter::regen` does, when it has the
    /// pool.
    pub(crate) fn regen(&mut self, pool: PoolId, per_second: Num, hz: u32) {
        self.change(pool, |meter| meter.regen(per_second, hz));
    }
}
