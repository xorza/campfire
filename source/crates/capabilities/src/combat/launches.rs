use bevy_ecs::resource::Resource;
use campfire_sim::{Position, StableId};

use crate::projectiles::projectile::{Flight, Payload};
use crate::units::unit_type::UnitType;

/// The projectiles that launch this tick: those ranged attacks fire, those actions deliver, and
/// those scripts launch. In a match with projectiles, `projectiles` inserts this list and takes
/// its launches in `CombatSet::Launch`; without, a ranged attack strikes at once. Not state: it
/// empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Launches {
    pub(crate) launches: Vec<Launch>,
    /// The casts named so far this tick.
    casts: u32,
}

/// A projectile of `unit_type` that `source` launches from `from`, flying as `flight` and
/// carrying `payload`, of the cast numbered `cast` this tick; an action's payload names its
/// group once the first projectile of the cast takes its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launch {
    pub(crate) source: StableId,
    pub(crate) from: Position,
    pub(crate) unit_type: UnitType,
    pub(crate) flight: Flight,
    pub(crate) payload: Payload,
    pub(crate) cast: u32,
}

impl Launches {
    /// A number for the next cast, which the projectiles it launches share.
    pub(crate) const fn cast(&mut self) -> u32 {
        self.casts += 1;
        self.casts
    }

    /// Empties it for the next tick.
    pub(crate) fn clear(&mut self) {
        self.launches.clear();
        self.casts = 0;
    }
}
