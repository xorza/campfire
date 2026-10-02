use bevy_ecs::component::Component;
use campfire_math::{Num, Vec3};
use campfire_sim::{SimComponent, StableId};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::units::action_id::ActionId;
use crate::values::damage_kind::DamageKind;

/// A projectile unit in flight: whose it is, how it flies, at its type's speed, what it carries,
/// and the units it struck, each once.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Projectile {
    source: StableId,
    flight: Flight,
    payload: Payload,
    struck: Vec<StableId>,
}

/// How a projectile flies: homing on a unit, or along a line, a unit vector on the ground or in
/// space, for `range` meters, with the unit its action aimed at, if one; `flown` meters of it so
/// far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Flight {
    Homing {
        target: StableId,
        flown: Num,
    },
    Line {
        direction: Vec3,
        flown: Num,
        range: Num,
        aimed: Option<StableId>,
    },
}

/// What a projectile carries: an attack's damage of `kind` and the roll it drew, or the action
/// at `rank` whose `on_hit` and `on_end` it runs, with the projectiles of its cast in `group`,
/// named by the first of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Payload {
    Attack {
        /// The weapon's action, which its damage names.
        action: ActionId,
        amount: Num,
        kind: DamageKind,
        roll: Num,
    },
    Action {
        action: ActionId,
        rank: u8,
        group: StableId,
    },
}

impl Projectile {
    /// `None` unless a line's distance flown is within its range, an attack's amount is not
    /// negative, and its roll is at least 0 and less than 1.
    pub(crate) fn new(source: StableId, flight: Flight, payload: Payload) -> Option<Projectile> {
        let flies = match flight {
            Flight::Homing { flown, .. } => flown >= Num::ZERO,
            Flight::Line { flown, range, .. } => Num::ZERO <= flown && flown <= range,
        };
        let carries = match payload {
            Payload::Attack { amount, roll, .. } => {
                amount >= Num::ZERO && Num::ZERO <= roll && roll < Num::ONE
            }
            Payload::Action { .. } => true,
        };
        (flies && carries).then_some(Projectile {
            source,
            flight,
            payload,
            struck: Vec::new(),
        })
    }

    pub(crate) const fn source(&self) -> StableId {
        self.source
    }

    pub(crate) const fn flight(&self) -> Flight {
        self.flight
    }

    pub(crate) const fn payload(&self) -> Payload {
        self.payload
    }

    /// Whether it struck `unit`.
    pub(crate) fn struck(&self, unit: StableId) -> bool {
        self.struck.contains(&unit)
    }

    pub(crate) fn strike(&mut self, unit: StableId) {
        debug_assert!(!self.struck(unit), "a projectile strikes a unit once");
        self.struck.push(unit);
    }

    /// Counts `flown` meters flown in all.
    pub(crate) fn fly_to(&mut self, flown: Num) {
        match &mut self.flight {
            Flight::Homing { flown: at, .. } => *at = flown,
            Flight::Line {
                flown: at, range, ..
            } => {
                debug_assert!(flown <= *range, "a line is flown within its range");
                *at = flown;
            }
        }
    }
}

impl SimComponent for Projectile {
    const NAME: &'static str = "projectiles.projectile";
}

/// A snapshot is untrusted, so a projectile `new` refuses, or one that struck a unit twice,
/// fails to decode.
impl<'de> Deserialize<'de> for Projectile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Projectile, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            source: StableId,
            flight: Flight,
            payload: Payload,
            struck: Vec<StableId>,
        }
        let Fields {
            source,
            flight,
            payload,
            struck,
        } = Fields::deserialize(deserializer)?;
        let mut projectile = Projectile::new(source, flight, payload)
            .ok_or_else(|| D::Error::custom("projectile out of its limits"))?;
        for unit in struck {
            if projectile.struck(unit) {
                return Err(D::Error::custom("a unit struck twice"));
            }
            projectile.strike(unit);
        }
        Ok(projectile)
    }
}
