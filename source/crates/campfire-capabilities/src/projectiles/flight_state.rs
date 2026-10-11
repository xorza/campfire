use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::projectiles::projectile::{Flight, Projectile};
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;

/// What changes of a projectile in flight: the meters it flew, and whether a teleport of the unit
/// it homes on disjointed it, which ends it in its next step. No client reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct FlightState {
    flown: Num,
    lost: bool,
}

impl FlightState {
    /// A flight none of which is flown.
    pub(crate) const START: FlightState = FlightState {
        flown: Num::ZERO,
        lost: false,
    };

    pub(crate) const fn flown(self) -> Num {
        self.flown
    }

    pub(crate) const fn lost(self) -> bool {
        self.lost
    }

    /// Counts `flown` meters flown in all, never less than it flew.
    pub(crate) fn fly_to(&mut self, flown: Num) {
        debug_assert!(flown >= self.flown, "a flight flies on");
        self.flown = flown;
    }

    /// Loses the unit it homes on.
    pub(crate) const fn disjoint(&mut self) {
        self.lost = true;
    }
}

impl SimComponent for FlightState {
    const NAME: &'static str = "projectiles.flight_state";

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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FlightState, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            flown: Num,
            lost: bool,
        }
        let Fields { flown, lost } = Fields::deserialize(deserializer)?;
        if flown < Num::ZERO {
            return Err(D::Error::custom("a flight of fewer than 0 meters"));
        }
        Ok(FlightState { flown, lost })
    }
}
