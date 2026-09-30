use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit's hit points, from 0 to a positive maximum. A unit at 0 dies at the end of the damage
/// step.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Health {
    current: Num,
    max: Num,
}

impl Health {
    /// Full health of `max`; `None` unless `max` is positive.
    pub const fn new(max: Num) -> Option<Health> {
        if max.to_bits() <= 0 {
            return None;
        }
        Some(Health { current: max, max })
    }

    pub const fn current(self) -> Num {
        self.current
    }

    pub const fn max(self) -> Num {
        self.max
    }

    pub const fn is_zero(self) -> bool {
        self.current.to_bits() == 0
    }

    /// Takes `amount`, which is not negative, down to 0 at most.
    pub(crate) fn take(&mut self, amount: Num) {
        debug_assert!(amount >= Num::ZERO, "damage is not negative");
        self.current = (self.current - amount).max(Num::ZERO);
    }
}

impl SimComponent for Health {
    const NAME: &'static str = "combat.health";
}

/// A snapshot is untrusted, so health outside 0 to a positive maximum fails to decode.
impl<'de> Deserialize<'de> for Health {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Health, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            current: Num,
            max: Num,
        }
        let Fields { current, max } = Fields::deserialize(deserializer)?;
        if max <= Num::ZERO || current < Num::ZERO || current > max {
            return Err(D::Error::custom("health outside 0 to a positive maximum"));
        }
        Ok(Health { current, max })
    }
}
