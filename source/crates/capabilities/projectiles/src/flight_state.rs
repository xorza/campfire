use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::DeliveryEntity;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::projectile::{Flight, Projectile};

/// What changes of a projectile in flight: the meters it flew, and whether a teleport of the unit
/// it homes on disjointed it, which ends it in its next step. No client reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FlightState {
    flown: Num,
    lost: bool,
}

impl FlightState {
    /// A flight none of which is flown.
    pub const START: Self = Self {
        flown: Num::ZERO,
        lost: false,
    };

    pub const fn flown(self) -> Num {
        self.flown
    }

    pub const fn lost(self) -> bool {
        self.lost
    }

    /// Counts `flown` meters flown in all, never less than it flew.
    pub fn fly_to(&mut self, flown: Num) {
        debug_assert!(flown >= self.flown, "a flight flies on");
        self.flown = flown;
    }

    /// Loses the unit it homes on.
    pub const fn disjoint(&mut self) {
        self.lost = true;
    }
}

impl SimComponent for FlightState {
    const NAME: &'static str = "projectiles.flight_state";
    type Host = DeliveryEntity;

    // Its projectile's: a line's flown within its range, and lost only when it homes.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<Projectile>(entity)
            .is_some_and(|projectile| match projectile.flight() {
                Flight::Homing { .. } => true,
                Flight::Line { range, .. } => !self.lost && self.flown <= range,
            })
    }
}

impl Replication for FlightState {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

/// A snapshot is untrusted, so a flight of fewer than 0 meters fails to decode.
impl<'de> Deserialize<'de> for FlightState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            flown: Num,
            lost: bool,
        }
        let Fields { flown, lost } = Fields::deserialize(deserializer)?;
        if flown < Num::ZERO {
            return Err(D::Error::custom("a flight of fewer than 0 meters"));
        }
        Ok(Self { flown, lost })
    }
}
