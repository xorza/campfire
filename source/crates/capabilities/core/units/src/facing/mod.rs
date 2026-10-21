use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::KindSpawn;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::turn_step::TurnStep;
use crate::unit_entity::UnitEntity;

/// The way a unit faces, in degrees from `x` towards `−z`, counter-clockwise seen from above, taken
/// modulo 360: its spawn's angle, a placed unit's or a build order's, as its box body turns by it;
/// a walker's turns toward the way it walks (04-capabilities/navigation.md, A walker faces its leg
/// and turns at its turn rate).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Facing(Num);

/// A facing turned toward a way, and whether it then faces within `Facing::WITHIN` of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub facing: Facing,
    pub faces: bool,
}

/// A full turn, in a `Num`'s bits.
const FULL: i64 = 360 << Num::FRAC_BITS;

/// Half a turn, in a `Num`'s bits.
const HALF: i64 = 180 << Num::FRAC_BITS;

impl Facing {
    /// `angle` degrees, taken modulo 360.
    pub const fn of(angle: Num) -> Self {
        Self(Num::from_bits(angle.to_bits().rem_euclid(FULL)))
    }

    /// The angle in degrees the unit of `at` faces: its spawn's core's, or, for a restored unit,
    /// its own facing's; 0 for one that faces none.
    pub(crate) fn angle_of<H>(at: &KindSpawn<'_, H>) -> Num {
        if let Some(core) = at.core() {
            return core.angle;
        }
        let facing = at.world().get::<Self>(at.entity());
        facing.map_or(Num::ZERO, |facing| facing.degrees())
    }

    /// The angle within which a walker faces its way and steps, 11.25°: Dota 2 lets a unit move
    /// once it faces within 0.2 radians of its way, about 11.46°, and this is the nearest angle
    /// of few bits.
    pub const WITHIN: Num = Num::from_bits(45 << (Num::FRAC_BITS - 2));

    /// Its degrees, at least 0 and below 360.
    pub const fn degrees(self) -> Num {
        self.0
    }

    /// The facing along `way`, its `x` and `z` on the ground plane: its angle in degrees,
    /// rounded once; `None` for a way of no length.
    pub fn along([x, z]: [Num; 2]) -> Option<Self> {
        if x == Num::ZERO && z == Num::ZERO {
            return None;
        }
        Some(Self::of((-z).atan2_degrees(x)))
    }

    /// The angle between it and `other` the shorter way round, from 0 to 180 degrees.
    pub const fn apart(self, other: Self) -> Num {
        let apart = (other.0.to_bits() - self.0.to_bits()).rem_euclid(FULL);
        Num::from_bits(if apart > HALF { FULL - apart } else { apart })
    }

    /// It turned toward `way` the shorter way round, counter-clockwise on a tie: by at most
    /// `step`'s degrees, or at once with no step; and whether it then faces within
    /// `Facing::WITHIN` of the way.
    pub fn toward(self, way: Self, step: Option<TurnStep>) -> Turn {
        // The turn to the way, from just past −180° to 180°.
        let mut left = Num::from_bits((way.0.to_bits() - self.0.to_bits()).rem_euclid(FULL));
        if left.to_bits() > HALF {
            left = Num::from_bits(left.to_bits() - FULL);
        }
        let turn = step.map_or(left, |step| left.clamp(-step.get(), step.get()));
        let rest = left - turn;
        Turn {
            facing: Self::of(self.0 + turn),
            faces: -Self::WITHIN <= rest && rest <= Self::WITHIN,
        }
    }
}

impl SimComponent for Facing {
    const NAME: &'static str = "units.facing";
    type Host = UnitEntity;

    fn check(&self, _: &World, _: Entity) -> bool {
        (0..FULL).contains(&self.0.to_bits())
    }
}

impl Replication for Facing {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

#[cfg(test)]
mod tests;
