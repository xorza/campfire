use std::num::NonZeroI64;

use campfire_actions::SlotKind;
use campfire_declarations::PerType;
use campfire_math::{Num, Rounding};
use campfire_sim::MatchRate;
use campfire_units::TurnStep;

use crate::turret::turret_data::TurretData;

pub(crate) mod aiming;
pub(crate) mod turret_angle;
pub(crate) mod turret_data;

/// A unit type's turret as a match runs it (04-capabilities/combat.md, A unit type may have a
/// turret): how far it turns a tick toward its target, and back toward its rest at half that,
/// how near its target's bearing it lets an attack start, and the kind of slot whose attacks it
/// carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turret {
    pub(crate) turn: TurnStep,
    pub(crate) rest: TurnStep,
    pub(crate) tolerance: Num,
    pub(crate) slot: SlotKind,
}

impl Turret {
    /// The turret of `data`, its attacks those of the slot kind `slot`, its turns a tick at
    /// `rate`, each rounded once and at least a bit, so a turret always turns.
    pub fn of(data: &TurretData, slot: SlotKind, rate: MatchRate) -> Self {
        let hz = NonZeroI64::from(rate.hz()).get();
        let step = |divisor: i64| {
            let step = data
                .turn_rate
                .checked_div_int(hz * divisor, Rounding::NearestEven)
                .expect("a turn rate over a tick rate fits");
            TurnStep::new(step.max(Num::EPSILON)).expect("a turn rate is positive")
        };
        Self {
            turn: step(1),
            rest: step(2),
            tolerance: data.tolerance_deg,
            slot,
        }
    }
}

impl PerType for Turret {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
mod tests;
