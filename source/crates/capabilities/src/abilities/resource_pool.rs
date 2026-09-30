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
