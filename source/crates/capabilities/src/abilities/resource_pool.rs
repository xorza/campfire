use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::values::meter::Meter;

/// What a unit's abilities cost, such as mana or energy: from 0 to a positive maximum.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourcePool(Meter);

impl ResourcePool {
    /// A full pool of `max`; `None` unless `max` is positive.
    pub const fn new(max: Num) -> Option<ResourcePool> {
        match Meter::new(max) {
            Some(meter) => Some(ResourcePool(meter)),
            None => None,
        }
    }

    pub const fn current(self) -> Num {
        self.0.current()
    }

    pub const fn max(self) -> Num {
        self.0.max()
    }

    /// Sets the maximum, as `Meter::set_max` does.
    pub(crate) fn set_max(&mut self, max: Num) {
        self.0.set_max(max);
    }

    /// Adds a tick's share of `per_second`, as `Meter::regen` does.
    pub(crate) fn regen(&mut self, per_second: Num, hz: u32) {
        self.0.regen(per_second, hz);
    }

    /// Restores `amount`, which is not negative, up to the maximum.
    pub(crate) fn restore(&mut self, amount: Num) {
        self.0.add(amount);
    }

    /// Spends `amount`, which a cast checked it can afford.
    pub(crate) fn spend(&mut self, amount: Num) {
        debug_assert!(
            Num::ZERO <= amount && amount <= self.current(),
            "a cast spends what it can afford"
        );
        self.0.take(amount);
    }
}

impl SimComponent for ResourcePool {
    const NAME: &'static str = "abilities.resource_pool";
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::abilities::resource_pool::ResourcePool;

    /// A pool of `max`, `spent` of it spent.
    pub fn spent_pool(max: Num, spent: Num) -> ResourcePool {
        let mut pool = ResourcePool::new(max).expect("a positive maximum");
        pool.spend(spent);
        pool
    }
}
