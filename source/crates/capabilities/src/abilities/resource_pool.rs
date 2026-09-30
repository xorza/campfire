use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// What a unit's abilities cost, such as mana or energy: from 0 to a positive maximum.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResourcePool {
    current: Num,
    max: Num,
}

impl ResourcePool {
    /// A full pool of `max`; `None` unless `max` is positive.
    pub const fn new(max: Num) -> Option<ResourcePool> {
        if max.to_bits() <= 0 {
            return None;
        }
        Some(ResourcePool { current: max, max })
    }

    pub const fn current(self) -> Num {
        self.current
    }

    pub const fn max(self) -> Num {
        self.max
    }

    /// Takes `amount`, which the pool holds.
    pub(crate) fn spend(&mut self, amount: Num) {
        debug_assert!(
            Num::ZERO <= amount && amount <= self.current,
            "a cast spends what it can afford"
        );
        self.current -= amount;
    }
}

impl SimComponent for ResourcePool {
    const NAME: &'static str = "abilities.resource_pool";
}

/// A snapshot is untrusted, so a pool outside 0 to a positive maximum fails to decode.
impl<'de> Deserialize<'de> for ResourcePool {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ResourcePool, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            current: Num,
            max: Num,
        }
        let Fields { current, max } = Fields::deserialize(deserializer)?;
        if max <= Num::ZERO || current < Num::ZERO || current > max {
            return Err(D::Error::custom(
                "resource pool outside 0 to a positive maximum",
            ));
        }
        Ok(ResourcePool { current, max })
    }
}
