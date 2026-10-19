use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::DeliveryEntity;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::projectile::{Flight, Projectile};

/// What changes of a projectile in flight. Of a homing or a line flight: the meters it flew, and
/// whether a teleport of the unit it homes on disjointed it, which ends it in its next step. Of a
/// curve: the meters it flew, the points it stood at, and its last control point, which follows
/// the unit it aimed at. No client reads it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FlightState {
    Flown { flown: Num, lost: bool },
    Curve { flown: Num, steps: u32, end: Vec3 },
}

impl FlightState {
    /// A homing or a line flight none of which is flown.
    pub const START: Self = Self::Flown {
        flown: Num::ZERO,
        lost: false,
    };

    /// The state of `flight` as it launches: a curve at none of its points, its end where it
    /// aimed.
    pub const fn of(flight: Flight) -> Self {
        match flight {
            Flight::Curve { to, .. } => Self::Curve {
                flown: Num::ZERO,
                steps: 0,
                end: to,
            },
            Flight::Homing { .. } | Flight::Line { .. } => Self::START,
        }
    }

    pub const fn flown(self) -> Num {
        match self {
            Self::Flown { flown, .. } | Self::Curve { flown, .. } => flown,
        }
    }

    /// Whether it lost the unit it homes on; a curve loses none.
    pub const fn is_lost(self) -> bool {
        match self {
            Self::Flown { lost, .. } => lost,
            Self::Curve { .. } => false,
        }
    }

    /// Counts `flown` meters flown in all by a homing or a line flight, never less than it flew.
    pub fn fly_to(&mut self, flown: Num) {
        let Self::Flown { flown: so_far, .. } = self else {
            panic!("a homing or a line flight flies its meters");
        };
        debug_assert!(flown >= *so_far, "a flight flies on");
        *so_far = flown;
    }

    /// Loses the unit a homing flight homes on.
    pub fn disjoint(&mut self) {
        let Self::Flown { lost, .. } = self else {
            panic!("a homing flight loses its unit");
        };
        *lost = true;
    }
}

impl SimComponent for FlightState {
    const NAME: &'static str = "projectiles.flight_state";
    type Host = DeliveryEntity;

    // Its projectile's: a line's flown within its range and lost only when it homes, and a
    // curve's at most at its last point, its end within the world's bound.
    fn check(&self, world: &World, entity: Entity) -> bool {
        world.get::<Projectile>(entity).is_some_and(|projectile| {
            match (projectile.flight(), *self) {
                (Flight::Homing { .. }, Self::Flown { .. }) => true,
                (Flight::Line { range, .. }, Self::Flown { flown, lost }) => {
                    !lost && flown <= range
                }
                (Flight::Curve { points, .. }, Self::Curve { steps, end, .. }) => {
                    steps <= points && end.within_bound()
                }
                _ => false,
            }
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
        enum Fields {
            Flown { flown: Num, lost: bool },
            Curve { flown: Num, steps: u32, end: Vec3 },
        }
        let state = match Fields::deserialize(deserializer)? {
            Fields::Flown { flown, lost } => Self::Flown { flown, lost },
            Fields::Curve { flown, steps, end } => Self::Curve { flown, steps, end },
        };
        if state.flown() < Num::ZERO {
            return Err(D::Error::custom("a flight of fewer than 0 meters"));
        }
        Ok(state)
    }
}
