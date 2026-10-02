use bevy_ecs::resource::Resource;
use campfire_sim::{Position, StableId};

use campfire_math::Num;

use crate::projectiles::projectile::Flight;
use crate::units::action_id::ActionId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;

/// The projectiles that launch this tick: those ranged attacks fire, those actions deliver, and
/// those scripts launch, which spawn in `CombatSet::Launch`. Not state: it empties within the
/// tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Launches {
    pub(crate) launches: Vec<Launch>,
    /// The casts named so far this tick.
    casts: u32,
}

/// A projectile of `unit_type` that `source` launches from `from`, flying as `flight` and
/// carrying `payload`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launch {
    pub(crate) source: StableId,
    pub(crate) from: Position,
    pub(crate) unit_type: UnitType,
    pub(crate) flight: Flight,
    pub(crate) payload: LaunchPayload,
}

/// What a launch carries: an attack's damage of `kind` and the roll it drew, or the action at
/// `rank` whose hooks it runs, of the cast numbered `cast` this tick, whose group is the first
/// projectile of the cast, known once that one spawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchPayload {
    Attack {
        action: ActionId,
        amount: Num,
        kind: DamageKind,
        roll: Num,
    },
    Action {
        action: ActionId,
        rank: u8,
        cast: u32,
    },
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
