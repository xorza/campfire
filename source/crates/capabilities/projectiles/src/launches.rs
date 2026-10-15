use std::slice;

use bevy_ecs::resource::Resource;
use campfire_declare::Scratch;
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_values::{ActionId, ActionStart, DamageKind, Rank, UnitType};

use crate::projectile::Flight;

/// The projectiles that launch this tick: those ranged attacks fire, those actions deliver, and
/// those scripts launch, which spawn in `Phase::Launch`. Not state: it empties within the
/// tick.
#[derive(Resource, Debug, Default)]
pub struct Launches {
    launches: Vec<Launch>,
    /// The casts named so far this tick.
    casts: u32,
}

/// A projectile of `unit_type` that `source` launches from `from`, flying as `flight` and
/// carrying `payload`; with `id`, the id the script that launched it took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launch {
    pub id: Option<StableId>,
    pub(crate) source: StableId,
    pub(crate) from: Vec3,
    pub(crate) unit_type: UnitType,
    pub(crate) flight: Flight,
    pub(crate) payload: LaunchPayload,
}

/// What a launch carries: an attack's damage of `kind`, the rank of its weapon's slot and the
/// roll it drew, or the action at `rank` whose hooks it runs, of the cast numbered `cast` this
/// tick, whose group is the first projectile of the cast, known once that one spawns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchPayload {
    Attack {
        action: ActionId,
        rank: Rank,
        amount: Num,
        kind: DamageKind,
        roll: Num,
    },
    Action {
        action: ActionId,
        rank: Rank,
        start: Option<ActionStart>,
        cast: u32,
    },
}

impl Launches {
    /// A number for the next cast, which the projectiles it launches share.
    pub(crate) const fn cast(&mut self) -> u32 {
        self.casts += 1;
        self.casts
    }

    pub const fn len(&self) -> usize {
        self.launches.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The launches, in the order they hold.
    pub fn iter(&self) -> slice::Iter<'_, Launch> {
        self.launches.iter()
    }

    pub fn push(&mut self, launch: Launch) {
        self.launches.push(launch);
    }

    pub(crate) fn extend(&mut self, launches: impl IntoIterator<Item = Launch>) {
        self.launches.extend(launches);
    }

    /// Orders the launches by their source's stable id, each source's in the order launched.
    pub(crate) fn sort_by_source(&mut self) {
        self.launches.sort_by_key(|launch| launch.source);
    }

    /// Empties it for the next tick.
    pub fn clear(&mut self) {
        self.launches.clear();
        self.casts = 0;
    }
}

impl Scratch for Launches {
    fn is_clear(&self) -> bool {
        self.launches.is_empty() && self.casts == 0
    }
}

impl<'a> IntoIterator for &'a Launches {
    type Item = &'a Launch;
    type IntoIter = slice::Iter<'a, Launch>;

    fn into_iter(self) -> slice::Iter<'a, Launch> {
        self.iter()
    }
}
