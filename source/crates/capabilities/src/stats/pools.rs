use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Capability, SimComponent, TickRate};
use serde::{Deserialize, Serialize};

use crate::declare::per_type::PerType;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnChange;
use crate::stats::meter::Meter;
use crate::stats::pool_book::PoolBook;
use crate::stats::pool_id::PoolId;
use crate::units::unit_entity::UnitEntity;

/// A unit's pools but its life, by pool id: each an amount from 0 to its maximum, none where its
/// type lists no such pool, and none in the mode's life pool, which `Life` holds, as only its
/// owner receives these. A fixed array, so copying a unit's pools, as a rollback does every
/// frame, allocates nothing.
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

    pub(crate) const fn meter(&self, pool: PoolId) -> Option<Meter> {
        self.0[pool.index()]
    }

    pub(crate) const fn meter_mut(&mut self, pool: PoolId) -> Option<&mut Meter> {
        self.0[pool.index()].as_mut()
    }

    /// Fills every pool it has.
    pub(crate) fn fill(&mut self) {
        for meter in self.0.iter_mut().flatten() {
            meter.fill();
        }
    }
}

impl SimComponent for Pools {
    const NAME: &'static str = "stats.pools";
    type Host = UnitEntity;

    // A fixed array of every pool place, whose meters' decode keeps each amount within its maximum;
    // a place the mode does not declare is never read, and the life pool's would hide from the
    // life its rules read. What a meter carries, regen adds: more than the match's rate leaves
    // would regenerate past a pool's rate.
    fn check(&self, world: &World, _: Entity) -> bool {
        let hz = world.resource::<TickRate>().hz().get();
        let life = world.get_resource::<PoolBook>().and_then(PoolBook::life);
        let no_life = life.is_none_or(|life| self.0[life.index()].is_none());
        no_life && self.0.iter().flatten().all(|meter| meter.settled(hz))
    }
}

impl Replication for Pools {
    const KIND: DataKind = DataKind::Pools;
    type Sending = SentOnChange;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::stats::pool_id::PoolId;
    use crate::stats::pools::Pools;

    impl Pools {
        /// The same pools with `amount` of `pool`, which they have, spent.
        #[must_use]
        pub fn spent(mut self, pool: PoolId, amount: Num) -> Pools {
            self.take(pool, amount);
            self
        }

        /// Takes `amount`, which is not negative, from `pool`, which it has, down to 0.
        pub(crate) fn take(&mut self, pool: PoolId, amount: Num) {
            let meter = self.meter_mut(pool);
            meter
                .expect("a unit spends from a pool it has")
                .take(amount);
        }
    }
}

impl PerType for Pools {
    const OWNER: Option<Capability> = Some(Capability::Stats);
    const SCRIPTS_ONLY: bool = false;
}
