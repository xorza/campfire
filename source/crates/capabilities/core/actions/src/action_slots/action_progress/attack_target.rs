use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// The unit a unit's attacks aim at, and what gave it (04-capabilities/control.md, A scanned
/// target holds only in reach).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackTarget {
    pub unit: StableId,
    pub source: TargetSource,
}

/// What gave a unit its attack target: an order, which it chases, or its idle scan, which it
/// drops once no weapon of it reaches the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetSource {
    Order,
    Scan,
}

impl AttackTarget {
    /// `unit`, as an order names it.
    pub const fn ordered(unit: StableId) -> Self {
        Self {
            unit,
            source: TargetSource::Order,
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
