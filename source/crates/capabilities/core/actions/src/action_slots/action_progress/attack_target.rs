use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// The unit a unit's attacks aim at, and what gave it (04-capabilities/control.md, A scanned
/// target holds only in reach).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackTarget {
    pub unit: StableId,
    pub source: TargetSource,
}

/// What gave a unit its attack target: an order, which it chases; its idle scan, which it
/// drops once no weapon of it reaches the target; or an ally's call to retaliate, which it chases
/// to its leash from where it stood, `from`, until the tick `until`, and then walks back
/// (combat-response.md, A retaliation's target is chased to its leash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetSource {
    Order,
    Scan,
    Retaliation { from: Vec3, until: Tick },
}

impl AttackTarget {
    /// `unit`, as an order names it.
    pub const fn ordered(unit: StableId) -> Self {
        Self {
            unit,
            source: TargetSource::Order,
        }
    }

    /// `unit`, as an ally's call to retaliate names it, to a unit that stands at `from`, chased
    /// until the tick `until`.
    pub const fn retaliated(unit: StableId, from: Vec3, until: Tick) -> Self {
        Self {
            unit,
            source: TargetSource::Retaliation { from, until },
        }
    }

    /// `unit`, as a unit's idle scan finds it.
    pub const fn scanned(unit: StableId) -> Self {
        Self {
            unit,
            source: TargetSource::Scan,
        }
    }
}
