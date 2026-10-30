use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;

/// Where a flier's flight takes it: up to its type's height, down to the ground, where the
/// capability that parks it holds it, down to the ground where its body has room, at a point
/// navigation finds and then holds, which it walks the flier to first, or toward `height` above
/// the ground by `climb` meters a tick, which that capability sets each tick as it takes the flier
/// off or lands it. State of `navigation`'s fliers, kept here, as `Destination` is, for the
/// capabilities above navigation to write (04-capabilities/airfields.md, A jet takes off and lands
/// on its runway; 04-capabilities/airfields.md, A jet takes off as a jet).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlightGoal {
    #[default]
    Height,
    Ground,
    Land,
    LandAt(Vec3),
    Toward {
        height: Num,
        climb: Num,
    },
}

impl FlightGoal {
    /// Whether the capability that parks its flier holds it, which moves only as it is forced.
    pub const fn held(self) -> bool {
        matches!(self, Self::Ground | Self::Toward { .. })
    }

    /// Whether its flier lands where its body has room, at a point navigation found or will find.
    pub const fn is_landing(self) -> bool {
        matches!(self, Self::Land | Self::LandAt(_))
    }
}

impl SimComponent for FlightGoal {
    const NAME: &'static str = "units.flight_goal";
    type Host = UnitEntity;

    // A point, a height within the world and a climb of none to its bound.
    fn check(&self, _: &World, _: Entity) -> bool {
        match *self {
            Self::Height | Self::Ground | Self::Land => true,
            Self::LandAt(point) => point.within_bound(),
            Self::Toward { height, climb } => {
                let within = Num::ZERO..=Vec3::BOUND;
                within.contains(&height) && within.contains(&climb)
            }
        }
    }
}

impl Replication for FlightGoal {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
