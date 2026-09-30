use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::values::meter::Meter;

/// A unit's hit points, from 0 to a positive maximum. A unit at 0 dies at the end of the damage
/// step.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Health(Meter);

impl Health {
    /// Full health of `max`; `None` unless `max` is positive.
    pub const fn new(max: Num) -> Option<Health> {
        match Meter::new(max) {
            Some(meter) => Some(Health(meter)),
            None => None,
        }
    }

    pub const fn current(self) -> Num {
        self.0.current()
    }

    pub const fn max(self) -> Num {
        self.0.max()
    }

    pub const fn is_zero(self) -> bool {
        self.0.is_empty()
    }

    pub(crate) const fn fill(&mut self) {
        self.0.fill();
    }

    /// Takes `amount` of damage, which is not negative, down to 0.
    pub(crate) fn take(&mut self, amount: Num) {
        self.0.take(amount);
    }
}

impl SimComponent for Health {
    const NAME: &'static str = "combat.health";
}
